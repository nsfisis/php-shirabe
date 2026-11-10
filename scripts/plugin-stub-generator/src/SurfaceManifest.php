<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node;
use PhpParser\Node\Expr\Array_;
use PhpParser\Node\Expr\PropertyFetch;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Expr\Variable;
use PhpParser\Node\Name;
use PhpParser\Node\Scalar\Int_;
use PhpParser\Node\Scalar\String_;
use PhpParser\Node\Stmt\Class_;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\NodeFinder;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitor\NameResolver;
use PhpParser\ParserFactory;

/**
 * Compares what the worker-side classes forward over RPC with what the Rust dispatchers answer.
 *
 * The forwarding side is read from the PHP classes the worker resolves proxied FQCNs to (the
 * generated stubs and the hand-written runtime classes): every `\ShirabeRpcRuntime::callRust` on
 * the instance handle, every `__shirabe_construct` and every `__shirabe_call_static`, plus the
 * `__shirabe_call_static` forwards of the guard classes. The
 * answering side is read from the `match` arms of the Rust dispatchers that serve them: the
 * functions `dispatchers.list` names for each class, `construct_entity` and `call_static_entity`.
 *
 * Each row is `<member><TAB><kind><TAB><status>`, sorted. `<kind>` is `constructor`, `method` or
 * `static`; `<status>` is `supported` (a dispatcher arm names the member), `unsupported` (the
 * member is forwarded and reaches the catch-all arm, whose answer is the explicit error) or
 * `orphan` (an arm names a member nothing forwards). A forwarded member is keyed `FQCN::name`; an
 * orphan method arm is keyed `<dispatcher function>::name`, since no class forwards it. A class
 * no entity is handed out as has no method rows; its subclasses carry the methods they inherit.
 *
 * Members spelled `__shirabe…` are the worker runtime's own protocol, and the property and clone
 * forwarders every stub carries are answered generically, so neither is part of the manifest.
 */
final class SurfaceManifest
{
    private const PROTOCOL_METHODS = ['__get', '__set', '__isset', '__unset'];

    /** What the R table answers for every entity, whatever class it crosses as. */
    private const GENERIC_METHODS = ['__get', '__set', '__isset', '__unset', '__shirabe_clone'];

    /**
     * FQCN => what its declaration forwards; `constructs` is null for a class declaring no
     * constructor of its own.
     *
     * @var array<string, array{guard: bool, parent: ?string, abstract: bool, methods: list<string>, statics: list<string>, constructs: ?bool}>
     */
    private array $classes = [];

    /**
     * @param list<string> $phpDirs the directories of worker-side classes, laid out by FQCN
     * @param array<string, ?list<string>> $dispatchers FQCN => the Rust functions answering its
     *                                                  instance methods; empty for a class no
     *                                                  entity is handed out as, null for a class
     *                                                  whose forwarding module is generated
     * @param list<string> $guardDirs the directories of the guard classes; only the static
     *                                methods a guard forwards are read from them, since every
     *                                other member of a guard is an explicit error
     */
    public function __construct(
        private readonly array $phpDirs,
        private readonly string $rustSourceDir,
        private readonly array $dispatchers,
        private readonly array $guardDirs = [],
    ) {
    }

    /**
     * The manifest text and the Rust module of the forwarding classes.
     *
     * @return array{manifest: string, forwarding: string}
     */
    public function render(): array
    {
        $this->readClasses();

        $errors = [];
        $forwarding = array_filter(
            $this->classes,
            fn (string $fqcn): bool => $this->isForwarding($fqcn),
            ARRAY_FILTER_USE_KEY,
        );
        foreach (array_keys($forwarding) as $fqcn) {
            if (!array_key_exists($fqcn, $this->dispatchers)) {
                $errors[] = "$fqcn forwards instance methods but has no entry in dispatchers.list";
            }
        }
        foreach (array_keys($this->dispatchers) as $fqcn) {
            if (!isset($forwarding[$fqcn])) {
                $errors[] = "stale entry in dispatchers.list: $fqcn forwards no instance method";
            }
        }
        if ($errors !== []) {
            throw new GenerationError($errors);
        }

        $generated = array_keys(array_filter(
            $this->dispatchers,
            static fn (?array $functions): bool => $functions === null,
        ));
        foreach ($generated as $fqcn) {
            $root = $this->root($fqcn);
            if ($this->dispatchers[$root] !== null) {
                $errors[] = "$fqcn is generated, but the class it descends from, $root, is not";
            }
        }
        if ($errors !== []) {
            throw new GenerationError($errors);
        }
        $modules = new ForwardingModules($this->classes, $generated, fn (string $fqcn): string => $this->root($fqcn));
        $impls = $modules->overrides(RustForwardingImpls::in($this->rustSourceDir));

        $rows = [];

        $served = [];
        foreach ($this->dispatchers as $functions) {
            foreach ($functions ?? [] as $function) {
                if (isset($served[$function])) {
                    continue;
                }
                $served[$function] = [];
                foreach (RustMatchArms::inFunction($this->rustSourceDir, $function, 'method_name.as_bytes()') as $alternative) {
                    $method = $this->single($function, $alternative);
                    if (!self::isProtocol($method)) {
                        $served[$function][$method] = true;
                    }
                }
            }
        }
        $forwardedBy = [];
        foreach ($this->dispatchers as $fqcn => $functions) {
            if ($functions === null) {
                if ($this->classes[$fqcn]['abstract']) {
                    continue;
                }
                foreach ($this->instanceSurface($fqcn) as $method) {
                    if (self::isProtocol($method)) {
                        continue;
                    }
                    $declaring = $this->declaring($fqcn, $method);
                    $supported = in_array(ForwardingModules::rustName($method), $impls[$declaring], true);
                    $rows[] = [$fqcn . '::' . $method, 'method', $supported ? 'supported' : 'unsupported'];
                }
                continue;
            }
            if ($functions === []) {
                continue;
            }
            foreach ($this->instanceSurface($fqcn) as $method) {
                if (self::isProtocol($method)) {
                    continue;
                }
                $supported = false;
                foreach ($functions as $function) {
                    $forwardedBy[$function][$method] = true;
                    $supported = $supported || isset($served[$function][$method]);
                }
                $rows[] = [$fqcn . '::' . $method, 'method', $supported ? 'supported' : 'unsupported'];
            }
        }
        foreach ($served as $function => $methods) {
            foreach (array_keys($methods) as $method) {
                if (!isset($forwardedBy[$function][$method])) {
                    $rows[] = [$function . '::' . $method, 'method', 'orphan'];
                }
            }
        }

        $constructible = [];
        foreach (RustMatchArms::inFunction($this->rustSourceDir, 'construct_entity', 'class.as_slice()') as $alternative) {
            $constructible[$this->single('construct_entity', $alternative)] = true;
        }
        $constructed = [];
        foreach (array_keys($this->classes) as $fqcn) {
            if (!$this->constructsEntity($fqcn)) {
                continue;
            }
            $constructed[$fqcn] = true;
            $rows[] = [$fqcn . '::__construct', 'constructor', isset($constructible[$fqcn]) ? 'supported' : 'unsupported'];
        }
        foreach (array_keys($constructible) as $fqcn) {
            if (!isset($constructed[$fqcn])) {
                $rows[] = [$fqcn . '::__construct', 'constructor', 'orphan'];
            }
        }

        $callable = [];
        foreach (RustMatchArms::inFunction($this->rustSourceDir, 'call_static_entity', '(class.as_slice(),method.as_slice())') as $alternative) {
            if (count($alternative) !== 2 || in_array(null, $alternative, true)) {
                throw new GenerationError(['call_static_entity: an arm that is not a (class, method) pair']);
            }
            $callable[$alternative[0] . '::' . $alternative[1]] = true;
        }
        $called = [];
        foreach ($this->classes as $fqcn => $class) {
            foreach ($class['statics'] as $method) {
                $member = $fqcn . '::' . $method;
                $called[$member] = true;
                $rows[] = [$member, 'static', isset($callable[$member]) ? 'supported' : 'unsupported'];
            }
        }
        foreach (array_keys($callable) as $member) {
            if (!isset($called[$member])) {
                $rows[] = [$member, 'static', 'orphan'];
            }
        }

        $lines = array_map(static fn (array $row): string => implode("\t", $row), $rows);
        sort($lines, SORT_STRING);

        return [
            'manifest' => "# Generated by scripts/plugin-stub-generator/surface-manifest; do not edit by hand.\n"
                . "# <member>\t<constructor|method|static>\t<supported|unsupported|orphan>\n"
                . implode("\n", $lines) . "\n",
            'forwarding' => $modules->render(),
        ];
    }

    /** Whether `$fqcn` is a stub or runtime class forwarding any instance method. */
    private function isForwarding(string $fqcn): bool
    {
        return !$this->classes[$fqcn]['guard'] && $this->instanceSurface($fqcn) !== [];
    }

    /** The forwarding class `$fqcn` descends from whose parent forwards nothing. */
    private function root(string $fqcn): string
    {
        while (($parent = $this->classes[$fqcn]['parent']) !== null && isset($this->classes[$parent]) && $this->isForwarding($parent)) {
            $fqcn = $parent;
        }

        return $fqcn;
    }

    /** The class, `$fqcn` or an ancestor, that declares the forwarder of `$method`. */
    private function declaring(string $fqcn, string $method): string
    {
        for ($class = $fqcn; $class !== null && isset($this->classes[$class]); $class = $this->classes[$class]['parent']) {
            if (in_array($method, $this->classes[$class]['methods'], true)) {
                return $class;
            }
        }
        throw new \LogicException("$fqcn does not forward $method");
    }

    /** Whether `new $fqcn(...)` builds a Rust-side entity. */
    private function constructsEntity(string $fqcn): bool
    {
        return !$this->classes[$fqcn]['abstract'] && $this->constructs($fqcn);
    }

    private static function isProtocol(string $method): bool
    {
        return str_starts_with($method, '__shirabe') || in_array($method, self::PROTOCOL_METHODS, true);
    }

    /** @param list<?string> $alternative */
    private function single(string $function, array $alternative): string
    {
        if (count($alternative) !== 1 || $alternative[0] === null) {
            throw new GenerationError(["$function: an arm naming more than one byte string"]);
        }

        return $alternative[0];
    }

    /** @return list<string> the instance methods `$fqcn` forwards, its own and its ancestors' */
    private function instanceSurface(string $fqcn): array
    {
        $methods = [];
        for ($class = $fqcn; $class !== null && isset($this->classes[$class]); $class = $this->classes[$class]['parent']) {
            foreach ($this->classes[$class]['methods'] as $method) {
                $methods[$method] = true;
            }
        }

        return array_keys($methods);
    }

    /** Whether `new $fqcn(...)` reaches `__shirabe_construct`, through its own or an inherited constructor. */
    private function constructs(string $fqcn): bool
    {
        for ($class = $fqcn; $class !== null && isset($this->classes[$class]); $class = $this->classes[$class]['parent']) {
            if ($this->classes[$class]['constructs'] !== null) {
                return $this->classes[$class]['constructs'];
            }
        }

        return false;
    }

    private function readClasses(): void
    {
        $parser = (new ParserFactory())->createForNewestSupportedVersion();
        $finder = new NodeFinder();
        $dirs = array_merge(
            array_map(static fn (string $dir): array => [$dir, false], $this->phpDirs),
            array_map(static fn (string $dir): array => [$dir, true], $this->guardDirs),
        );
        foreach ($dirs as [$dir, $isGuard]) {
            $iterator = new \RecursiveIteratorIterator(
                new \RecursiveDirectoryIterator($dir, \FilesystemIterator::SKIP_DOTS)
            );
            $paths = [];
            foreach ($iterator as $entry) {
                if ($entry->isFile() && str_ends_with($entry->getFilename(), '.php')) {
                    $paths[] = $entry->getPathname();
                }
            }
            sort($paths);
            foreach ($paths as $path) {
                $ast = $parser->parse(file_get_contents($path));
                $traverser = new NodeTraverser();
                $traverser->addVisitor(new NameResolver());
                $ast = $traverser->traverse($ast);
                foreach ($finder->findInstanceOf($ast, Class_::class) as $class) {
                    if ($class->namespacedName === null) {
                        continue;
                    }
                    $this->readClass($path, $class, $finder, $isGuard);
                }
            }
        }
    }

    private function readClass(string $path, Class_ $class, NodeFinder $finder, bool $isGuard): void
    {
        $fqcn = $class->namespacedName->toString();
        $entry = [
            'guard' => $isGuard,
            'parent' => $class->extends?->toString(),
            'abstract' => $class->isAbstract(),
            'methods' => [],
            'statics' => [],
            'constructs' => null,
        ];
        foreach ($class->getMethods() as $method) {
            if ($method->name->toString() === '__construct') {
                $entry['constructs'] = false;
            }
            foreach ($finder->find($method->stmts ?? [], self::isCallRust(...)) as $call) {
                $this->readCall($path, $fqcn, $method, $call, $entry);
            }
        }
        if ($isGuard) {
            if ($entry['statics'] === []) {
                return;
            }
            $entry = ['guard' => true, 'parent' => null, 'abstract' => $entry['abstract'], 'methods' => [], 'statics' => $entry['statics'], 'constructs' => null];
        }
        if (isset($this->classes[$fqcn])) {
            throw new GenerationError(["$path declares $fqcn a second time"]);
        }
        $this->classes[$fqcn] = $entry;
    }

    private static function isCallRust(Node $node): bool
    {
        return $node instanceof StaticCall
            && $node->class instanceof Name
            && $node->class->toString() === 'ShirabeRpcRuntime'
            && $node->name instanceof Node\Identifier
            && $node->name->toString() === 'callRust';
    }

    /** @param array{guard: bool, parent: ?string, abstract: bool, methods: list<string>, statics: list<string>, constructs: ?bool} $entry */
    private function readCall(string $path, string $fqcn, ClassMethod $method, StaticCall $call, array &$entry): void
    {
        $where = "$path: " . $fqcn . '::' . $method->name->toString();
        $handle = $call->args[0]->value ?? null;
        $name = $call->args[1]->value ?? null;
        if (!$name instanceof String_) {
            throw new GenerationError(["$where calls callRust without a literal method name"]);
        }
        $name = $name->value;
        if (
            $handle instanceof PropertyFetch
            && $handle->var instanceof Variable && $handle->var->name === 'this'
            && $handle->name instanceof Node\Identifier && $handle->name->toString() === '__rhandle'
        ) {
            if (!in_array($name, self::GENERIC_METHODS, true)) {
                $entry['methods'][] = $name;
            }
            return;
        }
        if (!$handle instanceof Int_ || $handle->value !== 0) {
            throw new GenerationError(["$where calls callRust on a handle other than its own or 0"]);
        }
        if ($name === '__shirabe_construct') {
            if ($method->name->toString() !== '__construct') {
                throw new GenerationError(["$where calls __shirabe_construct outside a constructor"]);
            }
            $entry['constructs'] = true;
        } elseif ($name === '__shirabe_call_static') {
            $arguments = $call->args[2]->value ?? null;
            $static = $arguments instanceof Array_ ? ($arguments->items[1]->value ?? null) : null;
            if (!$static instanceof String_) {
                throw new GenerationError(["$where forwards a static call without a literal method name"]);
            }
            $entry['statics'][] = $static->value;
        }
    }
}
