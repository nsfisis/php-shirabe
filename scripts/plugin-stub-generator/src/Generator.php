<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node\Name;
use PhpParser\Node\Stmt\Class_;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\Node\Stmt\Interface_;

/**
 * Emits the proxy stub files deterministically from the Composer sources and the classifier
 * report. Anything the emitter cannot faithfully proxy (by-ref or variadic parameters,
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

    /**
     * @param list<string> $targets
     * @param list<string> $runtimeProvided FQCNs of the hand-written dual-mode classes under
     *                                      php/runtime/; they may serve as stub base classes
     *                                      but must never be generation targets themselves
     */
    public function __construct(
        string $composerRoot,
        private readonly Report $report,
        private readonly array $targets,
        private readonly array $runtimeProvided = [],
    ) {
        $this->project = new Project($composerRoot);
        $this->printer = new NamePrinter();
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
        $staticMethods = $this->materializeStatics($file, $publicStatics, $nonPublicStatics);

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
     * Static methods read no instance state; their real implementation is materialized
     * verbatim so they run locally in the worker. Non-public static helpers they call
     * (through `self::`, `static::` or the class name) are materialized along with them.
     *
     * @param array<string, ClassMethod> $publicStatics
     * @param array<string, ClassMethod> $nonPublicStatics
     * @return list<string>
     */
    private function materializeStatics(SourceFile $file, array $publicStatics, array $nonPublicStatics): array
    {
        $texts = [];
        foreach ($publicStatics as $name => $method) {
            $texts[$name] = $file->verbatim($method->getStartLine(), $method->getEndLine());
        }
        $scan = array_values($texts);
        while ($scan !== []) {
            $text = array_shift($scan);
            foreach ($nonPublicStatics as $name => $method) {
                if (isset($texts[$name])) {
                    continue;
                }
                $receiver = '(?:self|static|' . preg_quote($file->classLike->name?->toString() ?? '', '/') . ')';
                if (preg_match('/(?<![\w$])' . $receiver . '::' . preg_quote($name, '/') . '\s*\(/', $text) === 1) {
                    $scan[] = $texts[$name] = $file->verbatim($method->getStartLine(), $method->getEndLine());
                }
            }
        }
        return array_values($texts);
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
        $params = [];
        $args = [];
        foreach ($method->params as $param) {
            $paramName = $param->var->name;
            if ($param->byRef) {
                $this->errors[] = "$fqcn::$name: by-ref parameter \$$paramName cannot be proxied yet";
            }
            if ($param->variadic) {
                $this->errors[] = "$fqcn::$name: variadic parameter \$$paramName cannot be proxied yet";
            }
            $rendered = '';
            if ($param->type !== null) {
                $rendered = $this->printer->renderType($param->type, $target) . ' ';
            }
            $rendered .= '$' . $paramName;
            if ($param->default !== null) {
                $rendered .= ' = ' . $this->printer->renderExpr($param->default, $target);
            }
            $params[] = $rendered;
            $args[] = '$' . $paramName;
        }

        $returnType = $this->printer->renderType($method->returnType, $target);
        $signature = "public function $name(" . implode(', ', $params) . ')'
            . ($returnType === '' ? '' : ": $returnType");
        $call = "\\ShirabeRpcRuntime::callRust(\$this->__rhandle, '$name', [" . implode(', ', $args) . '])';
        // `self`/`static` returns are fluent interfaces; the local stub itself is returned to
        // preserve identity instead of round-tripping the handle.
        $body = match ($returnType) {
            'void' => "        $call;",
            'self', 'static' => "        $call;\n        return \$this;",
            default => "        return $call;",
        };
        return "    $signature\n    {\n$body\n    }";
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
