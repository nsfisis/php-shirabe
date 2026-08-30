<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node;
use PhpParser\Node\Name;
use PhpParser\Node\Stmt\Class_;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\Node\Stmt\Interface_;
use PhpParser\NodeFinder;

/**
 * Emits the proxy stub files deterministically from the Composer sources and the classifier
 * report. Anything the emitter cannot faithfully proxy (variadic parameters, `func_get_args()`,
 * magic methods, non-public constants) fails generation instead of degrading silently.
 */
final class Generator
{
    private const BOILERPLATE = <<<'PHP'
        /** @var int */
        protected $__rhandle;
        /** @var int */
        protected $__epoch;

        /**
         * Binds a stub the registry built for an existing entity. Proxy instantiation bypasses
         * the constructor, which belongs to plugin code building a new entity instead.
         */
        public function __shirabeBind(int $rhandle, int $epoch): void
        {
            $this->__rhandle = $rhandle;
            $this->__epoch = $epoch;
        }

        public function __destruct()
        {
            \ShirabeRustObjectRegistry::release($this->__rhandle);
        }

        public function __shirabeRustHandleDescriptor(): array
        {
            return [
                '__rhandle' => $this->__rhandle,
                '__class' => static::class,
                '__epoch' => $this->__epoch,
            ];
        }

        public function __clone()
        {
            // PHP has already shallow-copied this stub, so both copies would point at one
            // entity and release it twice. The Rust side clones the entity instead, applying
            // whatever __clone semantics the real class defines, and this copy rebinds to the
            // fresh handle. Entities without clone semantics answer with an explicit error.
            [$this->__rhandle, $this->__epoch] = \ShirabeRpcRuntime::callRust($this->__rhandle, '__shirabeClone', []);
            \ShirabeRustObjectRegistry::adopt($this->__rhandle, $this);
        }
    PHP;

    private const PROPERTY_FORWARDERS = <<<'PHP'
        public function __get($name)
        {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, '__get', [$name]);
        }

        public function __set($name, $value): void
        {
            \ShirabeRpcRuntime::callRust($this->__rhandle, '__set', [$name, $value]);
        }

        public function __isset($name): bool
        {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, '__isset', [$name]);
        }

        public function __unset($name): void
        {
            \ShirabeRpcRuntime::callRust($this->__rhandle, '__unset', [$name]);
        }
    PHP;

    private Project $project;

    private NamePrinter $printer;

    /** @var list<string> */
    private array $errors = [];

    /** @var array<string, true> */
    private array $targetSet = [];

    /**
     * Emitted method records per stub class: fqcn => name => fingerprint. A subclass stub
     * omits overrides whose parameter list matches the record; the record therefore doubles
     * as the divergence detector for omitted overrides.
     *
     * @var array<string, array<string, list<array{string, bool, string, bool, bool}>>>
     */
    private array $surfaces = [];

    /** @var array<string, true> */
    private array $runtimeSet = [];

    /** @var array<string, true> */
    private array $guardExemptSet = [];

    private NodeFinder $nodeFinder;

    /**
     * @param list<string> $targets
     * @param list<string> $runtimeProvided FQCNs of the hand-written dual-mode classes under
     *                                      php/runtime/; they may serve as stub base classes
     *                                      but must never be generation targets themselves
     * @param list<string> $guardExempt     FQCNs the Rust side owns that the child still resolves
     *                                      to the real Composer class (guard-exemptions.list)
     */
    public function __construct(
        string $composerRoot,
        private readonly Report $report,
        private readonly array $targets,
        private readonly array $runtimeProvided = [],
        array $guardExempt = [],
    ) {
        $this->project = new Project($composerRoot);
        $this->printer = new NamePrinter();
        $this->nodeFinder = new NodeFinder();
        foreach ($guardExempt as $fqcn) {
            $this->guardExemptSet[$fqcn] = true;
        }
        foreach ($targets as $fqcn) {
            $this->targetSet[$fqcn] = true;
        }
        foreach ($runtimeProvided as $fqcn) {
            $this->runtimeSet[$fqcn] = true;
        }
    }

    /** @return array<string, string> relative stub path => file content */
    public function generate(): array
    {
        foreach ($this->runtimeProvided as $fqcn) {
            if (isset($this->targetSet[$fqcn])) {
                $this->errors[] = "$fqcn is both a stub target and provided by php/runtime/;"
                    . ' the runtime definition would be shadowed by the generated stub';
            }
        }
        $files = [];
        foreach ($this->targets as $fqcn) {
            $files[str_replace('\\', '/', $fqcn) . '.php'] = $this->emitClass($fqcn);
        }
        if ($this->errors !== []) {
            throw new GenerationError($this->errors);
        }
        return $files;
    }

    /**
     * The stub surface a runtime-provided (hand-written, dual-mode) base class exposes,
     * computed from the real Composer class the runtime file mirrors, so a generated subclass
     * stub can omit the methods it inherits — the same records emitClass builds for generated
     * parents.
     *
     * @return array<string, list<array{string, bool, string, bool, bool}>>
     */
    private function surfaceFromRealClass(string $fqcn): array
    {
        $file = $this->project->sourceFor($fqcn);
        $class = $file->classLike;
        if (!$class instanceof Class_) {
            $this->errors[] = "$fqcn is not a class";
            return [];
        }
        $surface = [];
        $parentFqcn = $class->extends === null ? null : SourceFile::resolvedName($class->extends);
        if ($parentFqcn !== null) {
            $surface = $this->surfaces[$parentFqcn] ?? $this->surfaceFromRealClass($parentFqcn);
        }
        foreach ($this->interfaceClosure($class) as $interface) {
            foreach ($interface->classLike->getMethods() as $method) {
                if ($method->isStatic()) {
                    continue;
                }
                $surface[$method->name->toString()] ??= $this->fingerprint($method, $file);
            }
        }
        foreach ($class->getMethods() as $method) {
            $name = $method->name->toString();
            if ($method->isStatic() || !$method->isPublic() || str_starts_with($name, '__')) {
                continue;
            }
            $surface[$name] = $this->fingerprint($method, $file);
        }
        return $surface;
    }

    private function emitClass(string $fqcn): string
    {
        $file = $this->project->sourceFor($fqcn);
        $class = $file->classLike;
        if (!$class instanceof Class_) {
            $this->errors[] = "$fqcn is not a class";
            return '';
        }
        $category = $this->report->category($fqcn);
        if (!in_array($category, ['rust-proxy', 'contract'], true)) {
            $this->errors[] = "$fqcn is classified as " . ($category ?? 'nothing')
                . '; only rust-proxy and contract classes can become proxy stubs';
        }

        $parentFqcn = $class->extends === null ? null : SourceFile::resolvedName($class->extends);
        $isRoot = $parentFqcn === null;
        if ($parentFqcn !== null && !isset($this->targetSet[$parentFqcn])) {
            if (isset($this->runtimeSet[$parentFqcn])) {
                $this->surfaces[$parentFqcn] ??= $this->surfaceFromRealClass($parentFqcn);
            } else {
                $this->errors[] = "$fqcn extends $parentFqcn, which is not a stub target";
                $isRoot = true;
            }
        }
        if (!$isRoot && !isset($this->surfaces[$parentFqcn])) {
            $this->errors[] = "$fqcn must come after its base class $parentFqcn in targets.list";
            $isRoot = true;
        }

        // Instance properties are entity state, so the stub declares none of them.
        $staticProperties = [];
        foreach ($class->getProperties() as $property) {
            if ($property->isPublic() && $property->isStatic()) {
                // A public static property reads no instance state; its real declaration is
                // materialized so it lives locally in the worker, like static methods.
                $staticProperties[] = $file->verbatim($property->getStartLine(), $property->getEndLine());
            }
        }

        // Constants are compile-time data with no entity behind them, so the declaration is
        // copied verbatim, visibility included: a local copy cannot diverge from the entity,
        // and nothing that was unreadable in the real class becomes readable here.
        $constants = [];
        foreach ($class->getConstants() as $constant) {
            $constants[] = $file->verbatim($constant->getStartLine(), $constant->getEndLine());
        }

        $publicStatics = [];
        $nonPublicStatics = [];
        $ownInstanceMethods = [];
        $constructor = null;
        foreach ($class->getMethods() as $method) {
            $name = $method->name->toString();
            if ($name === '__construct') {
                if ($method->isPublic()) {
                    $constructor = $method;
                }
                continue;
            }
            if (str_starts_with($name, '__')) {
                if ($method->isPublic() && !in_array($name, ['__toString', '__clone'], true)) {
                    $this->errors[] = "$fqcn::$name: magic methods cannot be proxied";
                }
                // __toString is an ordinary zero-argument call under a magic name and is
                // forwarded below. A real __clone declaration needs no counterpart here: the
                // boilerplate's forwarder delegates cloning to the entity, whose Rust-side
                // clone carries the declared semantics.
                if ($method->isPublic() && $name === '__toString') {
                    $ownInstanceMethods[$name] = $method;
                }
                continue;
            }
            if ($method->isStatic()) {
                if ($method->isPublic()) {
                    $publicStatics[$name] = $method;
                } else {
                    $nonPublicStatics[$name] = $method;
                }
            } elseif ($method->isPublic()) {
                $ownInstanceMethods[$name] = $method;
            }
        }
        [$staticMethods, $forwardedStatics] = $this->partitionStatics($file, $publicStatics, $nonPublicStatics);
        foreach ($forwardedStatics as $name => [$method, $reason]) {
            $staticMethods[] = $this->renderStaticForwarder($fqcn, $name, $method, $file, $reason);
        }

        $surface = [];
        $emitted = [];
        if ($isRoot) {
            foreach ($this->interfaceClosure($class) as $interface) {
                foreach ($interface->classLike->getMethods() as $method) {
                    $name = $method->name->toString();
                    if ($method->isStatic()) {
                        $this->errors[] = "$fqcn: static interface method $name is not supported";
                        continue;
                    }
                    $emitted[$name] ??= $method;
                }
            }
            // A concrete redeclaration wins over the interface signature (it may widen
            // defaults); it keeps the interface's position in the emission order.
            foreach ($ownInstanceMethods as $name => $method) {
                $emitted[$name] = $method;
            }
        } else {
            $surface = $this->surfaces[$parentFqcn];
            // Interfaces the subclass adds contribute the methods whose names are new
            // relative to the inherited stub surface.
            foreach ($this->interfaceClosure($class) as $interface) {
                foreach ($interface->classLike->getMethods() as $method) {
                    $name = $method->name->toString();
                    if ($method->isStatic()) {
                        $this->errors[] = "$fqcn: static interface method $name is not supported";
                        continue;
                    }
                    if (!isset($surface[$name])) {
                        $emitted[$name] ??= $method;
                    }
                }
            }
            foreach ($ownInstanceMethods as $name => $method) {
                if (isset($surface[$name])) {
                    $this->checkOmittedOverride($fqcn, $name, $method, $file, $surface[$name]);
                    unset($emitted[$name]);
                } else {
                    $emitted[$name] = $method;
                }
            }
        }

        $methodTexts = [];
        foreach ($emitted as $name => $method) {
            $methodTexts[] = $this->renderProxyMethod($fqcn, $name, $method, $file);
            $surface[$name] = $this->fingerprint($method, $file);
        }
        $this->surfaces[$fqcn] = $surface;

        $decl = ($class->isAbstract() ? 'abstract ' : '') . 'class ' . $class->name?->toString();
        if ($class->extends !== null) {
            $decl .= ' extends ' . $this->printer->renderName($class->extends, $file);
        }
        $interfaces = array_map(fn (Name $n): string => $this->printer->renderName($n, $file), $class->implements);
        if ($isRoot) {
            $interfaces[] = '\ShirabeRustStub';
        }
        if ($interfaces !== []) {
            $decl .= ' implements ' . implode(', ', $interfaces);
        }

        $members = [];
        if ($isRoot) {
            $members[] = self::BOILERPLATE;
            $members[] = self::PROPERTY_FORWARDERS;
        }
        if ($isRoot || $constructor !== null) {
            $members[] = $this->renderConstructor($fqcn, $constructor, $file);
        }
        if ($constants !== []) {
            $members[] = implode("\n", $constants);
        }
        if ($staticProperties !== []) {
            $members[] = implode("\n", $staticProperties);
        }
        $members = array_merge($members, $staticMethods, $methodTexts);
        $body = implode("\n\n", $members);

        $header = "// Generated by scripts/plugin-stub-generator; do not edit by hand.\n"
            . "// Proxy stub for $fqcn: the public surface forwards to the Rust-side entity over RPC.";
        $text = "<?php\n\n$header\n\nnamespace {$file->namespace};\n\n";
        $uses = $file->importsUsedBy($decl . "\n" . $body);
        if ($uses !== '') {
            $text .= $uses . "\n\n";
        }
        return $text . $decl . "\n{\n" . ($body === '' ? '' : $body . "\n") . "}\n";
    }

    /**
     * The constructor plugin code reaches when it writes `new SomeProxiedClass(...)`. The real
     * class's parameter list is reproduced and forwarded to the Rust side, which allocates the
     * entity and answers with its handle; classes whose entity it cannot build answer with an
     * explicit error naming the class.
     */
    private function renderConstructor(string $fqcn, ?ClassMethod $constructor, SourceFile $file): string
    {
        $params = [];
        $args = [];
        foreach ($constructor?->params ?? [] as $param) {
            $paramName = $param->var->name;
            if ($param->byRef) {
                $this->errors[] = "$fqcn::__construct: by-ref parameter \$$paramName cannot be proxied yet";
            }
            if ($param->variadic) {
                $this->errors[] = "$fqcn::__construct: variadic parameter \$$paramName cannot be proxied yet";
            }
            $rendered = '';
            if ($param->type !== null) {
                $rendered = $this->printer->renderType($param->type, $file) . ' ';
            }
            $rendered .= '$' . $paramName;
            if ($param->default !== null) {
                $rendered .= ' = ' . $this->printer->renderExpr($param->default, $file);
            }
            $params[] = $rendered;
            $args[] = '$' . $paramName;
        }
        $call = "\\ShirabeRpcRuntime::callRust(0, '__shirabeConstruct', [static::class, ["
            . implode(', ', $args) . ']])';
        return "    public function __construct(" . implode(', ', $params) . ")\n"
            . "    {\n"
            . "        [\$this->__rhandle, \$this->__epoch] = $call;\n"
            . "        \\ShirabeRustObjectRegistry::adopt(\$this->__rhandle, \$this);\n"
            . "    }";
    }

    /**
     * Splits the public static methods into the ones that run locally in the worker and the ones
     * that have to forward. A static method reads no instance state, so its real implementation is
     * materialized verbatim — unless it reaches something the worker does not have: a static
     * property, whose value the Rust side owns, or a class a guard shadows there. Non-public
     * static helpers a materialized method calls (through `self::`, `static::` or the class name)
     * are materialized along with it, and count towards what it reaches.
     *
     * @param array<string, ClassMethod> $publicStatics
     * @param array<string, ClassMethod> $nonPublicStatics
     * @return array{list<string>, array<string, array{ClassMethod, string}>} verbatim texts, and
     *         the methods to forward with the reason each of them cannot run locally
     */
    private function partitionStatics(SourceFile $file, array $publicStatics, array $nonPublicStatics): array
    {
        $texts = [];
        $forwarded = [];
        foreach ($publicStatics as $name => $method) {
            $blocker = $this->staticBlocker($file, $method, $nonPublicStatics);
            if ($blocker !== null) {
                $forwarded[$name] = [$method, $blocker];
                continue;
            }
            $texts[$name] = $file->verbatim($method->getStartLine(), $method->getEndLine());
        }
        $scan = array_values($texts);
        while ($scan !== []) {
            $text = array_shift($scan);
            foreach ($this->calledHelpers($file, $text, $nonPublicStatics) as $name => $method) {
                if (isset($texts[$name])) {
                    continue;
                }
                $scan[] = $texts[$name] = $file->verbatim($method->getStartLine(), $method->getEndLine());
            }
        }
        return [array_values($texts), $forwarded];
    }

    /**
     * The non-public static helpers a method body calls by name.
     *
     * @param  array<string, ClassMethod> $nonPublicStatics
     * @return array<string, ClassMethod>
     */
    private function calledHelpers(SourceFile $file, string $text, array $nonPublicStatics): array
    {
        $receiver = '(?:self|static|' . preg_quote($file->classLike->name?->toString() ?? '', '/') . ')';
        $found = [];
        foreach ($nonPublicStatics as $name => $method) {
            if (preg_match('/(?<![\w$])' . $receiver . '::' . preg_quote($name, '/') . '\s*\(/', $text) === 1) {
                $found[$name] = $method;
            }
        }
        return $found;
    }

    /**
     * Why a static method cannot be materialized into the worker, or null when it can. The answer
     * covers the transitive closure of the non-public static helpers it calls, since those are
     * materialized with it and reach whatever it reaches.
     *
     * @param array<string, ClassMethod> $nonPublicStatics
     */
    private function staticBlocker(SourceFile $file, ClassMethod $method, array $nonPublicStatics): ?string
    {
        $closure = [$method];
        $seen = [];
        $queue = [$method];
        while ($queue !== []) {
            $current = array_shift($queue);
            $text = $file->verbatim($current->getStartLine(), $current->getEndLine());
            foreach ($this->calledHelpers($file, $text, $nonPublicStatics) as $name => $helper) {
                if (isset($seen[$name])) {
                    continue;
                }
                $seen[$name] = true;
                $closure[] = $helper;
                $queue[] = $helper;
            }
        }
        foreach ($closure as $node) {
            foreach ($this->nodeFinder->findInstanceOf($node, Node\Expr\StaticPropertyFetch::class) as $fetch) {
                $property = $fetch->name instanceof Node\VarLikeIdentifier ? $fetch->name->toString() : '';
                return "reads the static property \$$property, whose value the Rust side owns";
            }
            $references = array_merge(
                $this->nodeFinder->findInstanceOf($node, Node\Expr\StaticCall::class),
                $this->nodeFinder->findInstanceOf($node, Node\Expr\ClassConstFetch::class),
                $this->nodeFinder->findInstanceOf($node, Node\Expr\New_::class),
            );
            foreach ($references as $reference) {
                if (!$reference->class instanceof Name) {
                    continue;
                }
                $fqcn = SourceFile::resolvedName($reference->class);
                if ($this->isGuardedInChild($fqcn)) {
                    return "references $fqcn, which a guard shadows in the worker";
                }
            }
        }
        return null;
    }

    /**
     * Whether the worker resolves this class to a guard rather than to executable code: the Rust
     * side owns it and neither a stub, a runtime definition nor a guard exemption stands in for
     * it, so materialized code reaching it would raise an explicit error at run time.
     */
    private function isGuardedInChild(string $fqcn): bool
    {
        if (isset($this->targetSet[$fqcn]) || isset($this->runtimeSet[$fqcn]) || isset($this->guardExemptSet[$fqcn])) {
            return false;
        }
        return in_array($this->report->category($fqcn), ['rust-proxy', 'rust-snapshot', 'unsupported'], true);
    }

    /** Interfaces implemented by the class, each interface preceding the ones it extends. */
    private function interfaceClosure(Class_ $class): array
    {
        $out = [];
        $seen = [];
        $visit = function (string $fqcn) use (&$visit, &$out, &$seen): void {
            if (isset($seen[$fqcn])) {
                return;
            }
            $seen[$fqcn] = true;
            if (interface_exists($fqcn, false) && (new \ReflectionClass($fqcn))->isInternal()) {
                // A PHP builtin interface (Countable, Stringable, ...) has no source file to
                // read; the class's own public methods already cover its surface.
                return;
            }
            $file = $this->project->sourceFor($fqcn);
            if (!$file->classLike instanceof Interface_) {
                $this->errors[] = "$fqcn is implemented as an interface but is not one";
                return;
            }
            $out[] = $file;
            foreach ($file->classLike->extends as $parent) {
                $visit(SourceFile::resolvedName($parent));
            }
        };
        foreach ($class->implements as $interface) {
            $visit(SourceFile::resolvedName($interface));
        }
        return $out;
    }

    private function renderProxyMethod(string $fqcn, string $name, ClassMethod $method, SourceFile $target): string
    {
        [$params, $args, $outPositions] = $this->renderParams($fqcn, $name, $method, $target);
        $returnType = $this->printer->renderType($method->returnType, $target);
        $signature = "public function $name(" . implode(', ', $params) . ')'
            . ($returnType === '' ? '' : ": $returnType");
        $body = $this->renderForwardingBody(
            "\\ShirabeRpcRuntime::callRust(\$this->__rhandle, '$name', %s%s)",
            $args,
            $outPositions,
            $this->inspectsArgCount($fqcn, $name, $method),
            $returnType,
        );
        return "    $signature\n    {\n$body\n    }";
    }

    /**
     * The counterpart of renderProxyMethod for a static method that cannot be materialized: the
     * call carries the class alongside the method name, since no handle identifies a receiver.
     * `$reason` is why it cannot, and is emitted with it so the generated file explains itself.
     */
    private function renderStaticForwarder(
        string $fqcn,
        string $name,
        ClassMethod $method,
        SourceFile $target,
        string $reason,
    ): string {
        [$params, $args, $outPositions] = $this->renderParams($fqcn, $name, $method, $target);
        if ($outPositions !== []) {
            $this->errors[] = "$fqcn::$name: a forwarded static method cannot take by-ref parameters yet";
        }
        $returnType = $this->printer->renderType($method->returnType, $target);
        $signature = "public static function $name(" . implode(', ', $params) . ')'
            . ($returnType === '' ? '' : ": $returnType");
        // self::class, not static::class: the forwarded statics stand for this class's own state,
        // and a plugin subclass redeclaring it is not modelled on the Rust side.
        $body = $this->renderForwardingBody(
            "\\ShirabeRpcRuntime::callRust(0, '__shirabeCallStatic', [self::class, '$name', %s]%s)",
            $args,
            $outPositions,
            $this->inspectsArgCount($fqcn, $name, $method),
            $returnType,
        );
        return "    // Forwarded rather than materialized: it $reason.\n"
            . "    $signature\n    {\n$body\n    }";
    }

    /**
     * The parameter list of a forwarded method and the argument expressions matching it.
     *
     * @return array{list<string>, list<string>, list<int>} parameters, arguments, by-ref positions
     */
    private function renderParams(string $fqcn, string $name, ClassMethod $method, SourceFile $target): array
    {
        $params = [];
        $args = [];
        $outPositions = [];
        foreach ($method->params as $position => $param) {
            $paramName = $param->var->name;
            if ($param->variadic) {
                $this->errors[] = "$fqcn::$name: variadic parameter \$$paramName cannot be proxied yet";
            }
            $rendered = '';
            if ($param->type !== null) {
                $rendered = $this->printer->renderType($param->type, $target) . ' ';
            }
            if ($param->byRef) {
                $rendered .= '&';
                $outPositions[] = $position;
            }
            $rendered .= '$' . $paramName;
            if ($param->default !== null) {
                $rendered .= ' = ' . $this->printer->renderExpr($param->default, $target);
            }
            $params[] = $rendered;
            $args[] = '$' . $paramName;
        }
        return [$params, $args, $outPositions];
    }

    /**
     * Whether the real method branches on how many arguments it was called with. The stub has to
     * reproduce that arity across the boundary — sending every declared parameter would make the
     * Rust side answer a call the plugin never made.
     */
    private function inspectsArgCount(string $fqcn, string $name, ClassMethod $method): bool
    {
        $inspects = false;
        foreach ($this->nodeFinder->findInstanceOf($method, Node\Expr\FuncCall::class) as $call) {
            if (!$call->name instanceof Name) {
                continue;
            }
            $function = strtolower($call->name->toString());
            if ($function === 'func_get_args') {
                $this->errors[] = "$fqcn::$name: func_get_args() cannot be proxied yet";
            }
            if ($function === 'func_num_args') {
                $inspects = true;
            }
        }
        return $inspects;
    }

    /**
     * The body every forwarding method shares: build the argument list, make the call, and copy
     * each by-ref parameter back out of the response.
     *
     * `$callTemplate` takes the argument-list expression and the trailing arguments of
     * `ShirabeRpcRuntime::callRust`, which only a by-ref parameter needs.
     *
     * @param list<string> $args
     * @param list<int>    $outPositions
     */
    private function renderForwardingBody(
        string $callTemplate,
        array $args,
        array $outPositions,
        bool $arityAware,
        string $returnType,
    ): string {
        $lines = [];
        $argsExpr = '[' . implode(', ', $args) . ']';
        if ($arityAware) {
            $lines[] = "        \$__args = $argsExpr;";
            $lines[] = '        array_splice($__args, func_num_args());';
            $argsExpr = '$__args';
        }
        if ($outPositions === []) {
            $call = sprintf($callTemplate, $argsExpr, '');
            // `self`/`static` returns are fluent interfaces; the local stub itself is returned to
            // preserve identity instead of round-tripping the handle.
            $lines[] = match ($returnType) {
                'void' => "        $call;",
                'self', 'static' => "        $call;\n        return \$this;",
                default => "        return $call;",
            };
            return implode("\n", $lines);
        }

        $tail = ', [' . implode(', ', $outPositions) . '], $__out';
        $call = sprintf($callTemplate, $argsExpr, $tail);
        $lines[] = '        $__out = [];';
        $lines[] = $returnType === 'void' ? "        $call;" : "        \$__result = $call;";
        foreach ($outPositions as $position) {
            // Absent when the callee left the parameter alone, which PHP reproduces by simply
            // not writing to it.
            $lines[] = "        if (array_key_exists($position, \$__out)) {";
            $lines[] = "            {$args[$position]} = \$__out[$position];";
            $lines[] = '        }';
        }
        if ($returnType === 'self' || $returnType === 'static') {
            $lines[] = '        return $this;';
        } elseif ($returnType !== 'void') {
            $lines[] = '        return $__result;';
        }
        return implode("\n", $lines);
    }

    /**
     * An override that is omitted from a subclass stub must take exactly the parameters the
     * inherited stub method declares: same names, arity, defaults and passing modes. Type
     * declarations are allowed to differ (overrides may widen them; the forwarding body is
     * type-agnostic either way).
     */
    private function checkOmittedOverride(
        string $fqcn,
        string $name,
        ClassMethod $method,
        SourceFile $file,
        array $inherited,
    ): void {
        if ($this->fingerprint($method, $file) !== $inherited) {
            $this->errors[] = "$fqcn::$name: override diverges from the inherited stub signature"
                . ' (names, arity, defaults or passing modes); it can no longer be omitted';
        }
    }

    /** @return list<array{string, bool, string, bool, bool}> */
    private function fingerprint(ClassMethod $method, SourceFile $file): array
    {
        $fingerprint = [];
        foreach ($method->params as $param) {
            $fingerprint[] = [
                $param->var->name,
                $param->default !== null,
                $param->default === null ? '' : $this->printer->renderExpr($param->default, $file),
                $param->byRef,
                $param->variadic,
            ];
        }
        return $fingerprint;
    }
}
