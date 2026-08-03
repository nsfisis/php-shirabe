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
 * magic methods, public properties, non-public constants) fails generation instead of
 * degrading silently.
 */
final class Generator
{
    private const BOILERPLATE = <<<'PHP'
        /** @var int */
        protected $__rhandle;
        /** @var int */
        protected $__epoch;

        public function __construct(int $rhandle = 0, int $epoch = 0)
        {
            if (func_num_args() < 2) {
                // Constructing the class from plugin code (a common idiom for e.g. `new BufferIO()`)
                // is an open question of the plugin design; only proxy instantiation passes a
                // Rust handle. Fail with a diagnosable message instead of an ArgumentCountError.
                throw new \RuntimeException(
                    'Shirabe does not support constructing ' . static::class . ' inside the plugin process yet'
                );
            }
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

    /** @param list<string> $targets */
    public function __construct(
        string $composerRoot,
        private readonly Report $report,
        private readonly array $targets,
    ) {
        $this->project = new Project($composerRoot);
        $this->printer = new NamePrinter();
        foreach ($targets as $fqcn) {
            $this->targetSet[$fqcn] = true;
        }
    }

    /** @return array<string, string> relative stub path => file content */
    public function generate(): array
    {
        $files = [];
        foreach ($this->targets as $fqcn) {
            $files[str_replace('\\', '/', $fqcn) . '.php'] = $this->emitClass($fqcn);
        }
        if ($this->errors !== []) {
            throw new GenerationError($this->errors);
        }
        return $files;
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

        $parentFqcn = $class->extends === null ? null : $this->resolvedName($class->extends);
        $isRoot = $parentFqcn === null;
        if ($parentFqcn !== null && !isset($this->targetSet[$parentFqcn])) {
            $this->errors[] = "$fqcn extends $parentFqcn, which is not a stub target";
            $isRoot = true;
        }
        if (!$isRoot && !isset($this->surfaces[$parentFqcn])) {
            $this->errors[] = "$fqcn must come after its base class $parentFqcn in targets.list";
            $isRoot = true;
        }

        foreach ($class->getProperties() as $property) {
            if ($property->isPublic()) {
                $this->errors[] = "$fqcn declares a public property; a proxy stub cannot forward property access";
            }
        }

        $constants = [];
        foreach ($class->getConstants() as $constant) {
            if (!$constant->isPublic()) {
                $this->errors[] = "$fqcn declares a non-public constant; materializing it is not supported";
                continue;
            }
            $constants[] = $file->verbatim($constant->getStartLine(), $constant->getEndLine());
        }

        $publicStatics = [];
        $nonPublicStatics = [];
        $ownInstanceMethods = [];
        foreach ($class->getMethods() as $method) {
            $name = $method->name->toString();
            if ($name === '__construct') {
                continue;
            }
            if (str_starts_with($name, '__')) {
                if ($method->isPublic()) {
                    $this->errors[] = "$fqcn::$name: magic methods cannot be proxied";
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
            if ($class->implements !== []) {
                $this->errors[] = "$fqcn adds interfaces to a stub base class; this is not supported yet";
            }
            foreach ($ownInstanceMethods as $name => $method) {
                if (isset($surface[$name])) {
                    $this->checkOmittedOverride($fqcn, $name, $method, $file, $surface[$name]);
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
        if ($isRoot) {
            $interfaces = array_map(fn (Name $n): string => $this->printer->renderName($n, $file), $class->implements);
            $interfaces[] = '\ShirabeRustStub';
            $decl .= ' implements ' . implode(', ', $interfaces);
        }

        $members = [];
        if ($isRoot) {
            $members[] = self::BOILERPLATE;
        }
        if ($constants !== []) {
            $members[] = implode("\n", $constants);
        }
        $members = array_merge($members, $staticMethods, $methodTexts);
        $body = implode("\n\n", $members);

        $header = "// Generated by scripts/plugin-stub-generator; do not edit by hand.\n"
            . "// Proxy stub for $fqcn: the public surface forwards to the Rust-side entity over RPC.";
        $text = "<?php\n\n$header\n\nnamespace {$file->namespace};\n\n";
        $uses = $this->usedImports($file, $decl . "\n" . $body);
        if ($uses !== '') {
            $text .= $uses . "\n\n";
        }
        return $text . $decl . "\n{\n" . ($body === '' ? '' : $body . "\n") . "}\n";
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
            $file = $this->project->sourceFor($fqcn);
            if (!$file->classLike instanceof Interface_) {
                $this->errors[] = "$fqcn is implemented as an interface but is not one";
                return;
            }
            $out[] = $file;
            foreach ($file->classLike->extends as $parent) {
                $visit($this->resolvedName($parent));
            }
        };
        foreach ($class->implements as $interface) {
            $visit($this->resolvedName($interface));
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

    /** The original file's imports, restricted to names the emitted stub actually uses. */
    private function usedImports(SourceFile $file, string $emittedText): string
    {
        $kept = [];
        foreach ($file->aliases as $alias => $fqcn) {
            if (preg_match('/(?<![\\\\$\w])' . preg_quote($alias, '/') . '\b/', $emittedText) === 1) {
                $kept[] = 'use ' . $fqcn
                    . (str_ends_with($fqcn, '\\' . $alias) || $fqcn === $alias ? '' : " as $alias") . ';';
            }
        }
        return implode("\n", $kept);
    }

    private function resolvedName(Name $name): string
    {
        $resolved = $name->getAttribute('resolvedName');
        return $resolved instanceof Name ? $resolved->toString() : $name->toString();
    }
}
