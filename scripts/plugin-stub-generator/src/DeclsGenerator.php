<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node;
use PhpParser\Node\Identifier;
use PhpParser\Node\IntersectionType;
use PhpParser\Node\Name;
use PhpParser\Node\NullableType;
use PhpParser\Node\Param;
use PhpParser\Node\Stmt\Class_;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\Node\Stmt\Interface_;
use PhpParser\Node\UnionType;
use PhpParser\NodeFinder;

/**
 * Emits `crates/shirabe/php-decls/`: one JSON file per Composer class or interface the Rust
 * bindings answer for or stand in for, describing its PHP surface for the Rust side's binding
 * macros to read at compile time.
 *
 * The types described are the interfaces of the classifier report and the classes that forward
 * members to the Rust side, together with every Composer supertype of them. A Composer type is one
 * whose source lives under `composer/src/`; PHP's builtin types and vendor types are never
 * described, and their members count only where a Composer type declares them too.
 *
 * Each file holds the type's surface as seen from it: the public methods (static ones included)
 * it declares or inherits from a Composer supertype, its constructor, and the public and protected
 * properties. A method's `declaredIn` is its topmost Composer declaration: the interface that
 * declares it, if any, and the topmost class otherwise, since the Rust side answers a method
 * through the trait or the type of that declaration, whichever subclass the receiver is. Its
 * parameters are those of the nearest declaration, the one a stub of the type reproduces.
 */
final class DeclsGenerator
{
    /** Magic methods that are ordinary calls under a magic name. */
    private const ORDINARY_MAGIC = ['__toString'];

    private NodeFinder $finder;

    /** @var list<string> */
    private array $errors = [];

    /** @var array<string, array<string, ClassMethod>> */
    private array $ownMethods = [];

    /** @var array<string, ?SourceFile> fqcn => source, or null for a non-Composer type */
    private array $composerSources = [];

    public function __construct(
        private readonly Project $project,
        private readonly string $composerRoot,
        private readonly Report $report,
    ) {
        $this->finder = new NodeFinder();
    }

    /** @return array<string, string> path relative to php-decls/ => file content */
    public function generate(): array
    {
        $seeds = [];
        foreach ($this->report->classes() as $fqcn) {
            if ($this->report->kind($fqcn) === 'interface' || $this->forwardedMembers($fqcn) !== []) {
                if ($this->composerSource($fqcn) !== null) {
                    $seeds[] = $fqcn;
                }
            }
        }
        $types = [];
        $queue = $seeds;
        while ($queue !== []) {
            $fqcn = array_shift($queue);
            if (isset($types[$fqcn])) {
                continue;
            }
            $types[$fqcn] = true;
            foreach ($this->directSupertypes($fqcn) as $supertype) {
                $queue[] = $supertype;
            }
        }
        $fqcns = array_keys($types);
        sort($fqcns, SORT_STRING);

        $files = [];
        foreach ($fqcns as $fqcn) {
            $decl = $this->describe($fqcn);
            $files[str_replace('\\', '/', $fqcn) . '.json'] = json_encode(
                $decl,
                JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
            ) . "\n";
        }
        if ($this->errors !== []) {
            throw new GenerationError($this->errors);
        }

        return $files;
    }

    /** @return array<string, mixed> */
    private function describe(string $fqcn): array
    {
        $file = $this->composerSource($fqcn);
        $classLike = $file->classLike;
        $isClass = $classLike instanceof Class_;
        if (!$isClass && !$classLike instanceof Interface_) {
            $this->errors[] = "$fqcn is neither a class nor an interface";
            return [];
        }
        $forwarding = $isClass && $this->forwardedMembers($fqcn) !== [];

        $decl = [
            'fqcn' => $fqcn,
            'rustModule' => ForwardingModules::modulePath($fqcn),
            'kind' => $isClass ? 'class' : 'interface',
        ];
        if ($isClass) {
            $decl['abstract'] = $classLike->isAbstract();
            $decl['parent'] = $this->composerParent($fqcn);
        }
        $decl['interfaces'] = $this->composerInterfaces($fqcn);
        $decl['treatment'] = $this->report->treatment($fqcn);

        $methods = [];
        foreach ($this->methodNames($fqcn) as $name) {
            $methods[] = $this->describeMethod($fqcn, $name, $forwarding);
        }
        $decl['methods'] = $methods;
        if ($isClass) {
            $decl['constructor'] = $this->describeConstructor($fqcn, $forwarding);
            $decl['properties'] = $this->describeProperties($fqcn);
        }
        if ($forwarding) {
            $this->checkForwardedCovered($fqcn, $methods, $decl['constructor']);
        }

        return $decl;
    }

    /**
     * The public methods of the surface of a type, by name. A method a Composer type declares
     * counts; one only a builtin or vendor supertype declares does not.
     *
     * @return list<string>
     */
    private function methodNames(string $fqcn): array
    {
        $names = [];
        foreach ($this->composerClosure($fqcn) as $type) {
            foreach ($this->ownMethods($type) as $name => $method) {
                if (!$method->isPublic() || $name === '__construct') {
                    continue;
                }
                if (str_starts_with($name, '__') && !in_array($name, self::ORDINARY_MAGIC, true)) {
                    continue;
                }
                $names[$name] = true;
            }
        }
        $names = array_keys($names);
        sort($names, SORT_STRING);

        return $names;
    }

    /** @return array<string, mixed> */
    private function describeMethod(string $fqcn, string $name, bool $forwarding): array
    {
        $method = $this->nearestDeclaration($fqcn, $name);
        $doc = $this->docTags($method);
        $row = [
            'name' => $name,
            'rust' => ForwardingModules::rustName($name),
            'declaredIn' => $this->topmostDeclaration($fqcn, $name),
            'static' => $method->isStatic(),
            'params' => $this->describeParams($method, $doc),
            'returnType' => $this->renderType($method->returnType),
            'docReturnType' => $doc['return'],
            'funcNumArgs' => $this->callsFuncNumArgs($method),
        ];
        if ($forwarding) {
            $row += $this->forwarding($fqcn, $name, $method->isStatic() ? 'static' : 'method');
        }

        return $row;
    }

    /** @return ?array<string, mixed> */
    private function describeConstructor(string $fqcn, bool $forwarding): ?array
    {
        for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
            $method = $this->ownMethods($type)['__construct'] ?? null;
            if ($method === null) {
                continue;
            }
            $row = [
                'declaredIn' => $type,
                'public' => $method->isPublic(),
                'params' => $this->describeParams($method, $this->docTags($method)),
                'funcNumArgs' => $this->callsFuncNumArgs($method),
            ];
            if ($forwarding) {
                $row += $this->forwarding($fqcn, '__construct', 'constructor');
            }

            return $row;
        }

        // The implicit constructor PHP gives a class declaring none.
        $row = ['declaredIn' => null, 'public' => true, 'params' => [], 'funcNumArgs' => false];
        if ($forwarding) {
            $row += $this->forwarding($fqcn, '__construct', 'constructor');
        }

        return $row;
    }

    /**
     * The public and protected properties, the constructor's promoted parameters included, each
     * from its nearest declaration.
     *
     * @return list<array<string, mixed>>
     */
    private function describeProperties(string $fqcn): array
    {
        $properties = [];
        for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
            $classLike = $this->composerSource($type)->classLike;
            foreach ($classLike->getProperties() as $property) {
                if ($property->isPrivate()) {
                    continue;
                }
                $docType = $this->docVarType($property->getDocComment()?->getText());
                foreach ($property->props as $prop) {
                    $properties[$prop->name->toString()] ??= [
                        'name' => $prop->name->toString(),
                        'declaredIn' => $type,
                        'visibility' => $property->isPublic() ? 'public' : 'protected',
                        'static' => $property->isStatic(),
                        'readonly' => $property->isReadonly(),
                        'type' => $this->renderType($property->type),
                        'docType' => $docType,
                    ];
                }
            }
            $constructor = $classLike->getMethod('__construct');
            $doc = $constructor === null ? null : $this->docTags($constructor);
            foreach ($constructor?->params ?? [] as $param) {
                if (!$param->isPromoted() || ($param->flags & Class_::MODIFIER_PRIVATE) !== 0) {
                    continue;
                }
                $name = $param->var->name;
                $properties[$name] ??= [
                    'name' => $name,
                    'declaredIn' => $type,
                    'visibility' => ($param->flags & Class_::MODIFIER_PROTECTED) !== 0 ? 'protected' : 'public',
                    'static' => false,
                    'readonly' => ($param->flags & Class_::MODIFIER_READONLY) !== 0,
                    'type' => $this->renderType($param->type),
                    'docType' => $doc['params'][$name] ?? null,
                ];
            }
        }
        ksort($properties, SORT_STRING);

        return array_values($properties);
    }

    /**
     * @param array{params: array<string, string>, return: ?string} $doc
     * @return list<array<string, mixed>>
     */
    private function describeParams(ClassMethod $method, array $doc): array
    {
        $params = [];
        foreach ($method->params as $position => $param) {
            $name = $param->var->name;
            $params[] = [
                'name' => $name,
                'position' => $position,
                'byRef' => $param->byRef,
                'variadic' => $param->variadic,
                'nullable' => $this->acceptsNull($param),
                'type' => $this->renderType($param->type),
                'docType' => $doc['params'][$name] ?? null,
            ];
        }

        return $params;
    }

    /**
     * Whether null can arrive in the parameter: it is untyped, its type admits null, or it
     * defaults to null (PHP's implicit nullability).
     */
    private function acceptsNull(Param $param): bool
    {
        if ($param->type === null) {
            return true;
        }
        if ($param->default instanceof Node\Expr\ConstFetch && strtolower($param->default->name->toString()) === 'null') {
            return true;
        }

        return $this->typeAdmitsNull($param->type);
    }

    private function typeAdmitsNull(Node $type): bool
    {
        if ($type instanceof NullableType) {
            return true;
        }
        if ($type instanceof UnionType) {
            foreach ($type->types as $member) {
                if ($this->typeAdmitsNull($member)) {
                    return true;
                }
            }
            return false;
        }

        return $type instanceof Identifier && in_array(strtolower($type->toString()), ['null', 'mixed'], true);
    }

    /** A native type with every class name fully qualified (`\Composer\Package\PackageInterface`). */
    private function renderType(?Node $type): ?string
    {
        if ($type === null) {
            return null;
        }
        if ($type instanceof Identifier) {
            return $type->toString();
        }
        if ($type instanceof NullableType) {
            return '?' . $this->renderType($type->type);
        }
        if ($type instanceof UnionType || $type instanceof IntersectionType) {
            $glue = $type instanceof UnionType ? '|' : '&';
            return implode($glue, array_map(function (Node $member) use ($type): string {
                $rendered = $this->renderType($member);
                return $type instanceof UnionType && $member instanceof IntersectionType ? "($rendered)" : $rendered;
            }, $type->types));
        }
        if ($type instanceof Name) {
            return $type->isSpecialClassName() ? $type->toString() : '\\' . SourceFile::resolvedName($type);
        }
        $this->errors[] = 'unsupported type node ' . get_class($type);

        return null;
    }

    /**
     * The `@param` and `@return` types of a doc comment, as written.
     *
     * @return array{params: array<string, string>, return: ?string}
     */
    private function docTags(ClassMethod $method): array
    {
        $tags = ['params' => [], 'return' => null];
        $text = $method->getDocComment()?->getText();
        if ($text === null) {
            return $tags;
        }
        foreach ($this->docLines($text) as $line) {
            if (preg_match('/^@(?:phpstan-|psalm-)?param\s+(.*)$/', $line, $m) === 1) {
                $type = self::leadingType($m[1]);
                $rest = ltrim(substr($m[1], strlen($type ?? '')));
                if (preg_match('/^&?(?:\.\.\.)?\$(\w+)/', $rest, $n) === 1 && $type !== null) {
                    // A phpstan- or psalm- tag refines the plain one.
                    if (!isset($tags['params'][$n[1]]) || !str_starts_with($line, '@param')) {
                        $tags['params'][$n[1]] = $type;
                    }
                }
            } elseif (preg_match('/^@(?:phpstan-|psalm-)?return\s+(.*)$/', $line, $m) === 1) {
                $type = self::leadingType($m[1]);
                if ($type !== null && ($tags['return'] === null || !str_starts_with($line, '@return'))) {
                    $tags['return'] = $type;
                }
            }
        }

        return $tags;
    }

    private function docVarType(?string $text): ?string
    {
        if ($text === null) {
            return null;
        }
        $found = null;
        foreach ($this->docLines($text) as $line) {
            if (preg_match('/^@(?:phpstan-|psalm-)?var\s+(.*)$/', $line, $m) === 1) {
                $type = self::leadingType($m[1]);
                if ($type !== null && ($found === null || !str_starts_with($line, '@var'))) {
                    $found = $type;
                }
            }
        }

        return $found;
    }

    /**
     * The lines of a doc comment, without the comment markers, a tag continued on the following
     * lines (a multi-line array shape) joined into one.
     *
     * @return list<string>
     */
    private function docLines(string $text): array
    {
        $lines = [];
        foreach (preg_split('/\R/', $text) as $raw) {
            $line = trim(preg_replace('#^\s*(?:/\*\*|\*/|\*)?#', '', preg_replace('#\*/\s*$#', '', $raw)));
            if ($line === '') {
                continue;
            }
            if (!str_starts_with($line, '@') && $lines !== [] && self::openBrackets(end($lines)) > 0) {
                $lines[count($lines) - 1] .= ' ' . $line;
                continue;
            }
            $lines[] = $line;
        }

        return $lines;
    }

    /** The type at the start of a tag's text: up to the first whitespace outside brackets and quotes. */
    private static function leadingType(string $text): ?string
    {
        $depth = 0;
        $quote = null;
        $length = strlen($text);
        for ($i = 0; $i < $length; $i++) {
            $char = $text[$i];
            if ($quote !== null) {
                if ($char === $quote) {
                    $quote = null;
                }
                continue;
            }
            if ($char === '"' || $char === "'") {
                $quote = $char;
            } elseif (str_contains('<({[', $char)) {
                $depth++;
            } elseif (str_contains('>)}]', $char) && !($char === '>' && $i > 0 && $text[$i - 1] === '-')) {
                $depth--;
            } elseif ($depth === 0 && ctype_space($char)) {
                // `A | B` and `A, B` inside a shape are joined by the bracket depth; at the top
                // level, a `|` next to the whitespace continues the type.
                $next = ltrim(substr($text, $i));
                if (str_starts_with($next, '|') || ($i > 0 && $text[$i - 1] === '|')) {
                    continue;
                }
                break;
            }
        }
        $type = trim(substr($text, 0, $i));
        if ($type === '' || str_starts_with($type, '$')) {
            return null;
        }

        return preg_replace('/\s*\|\s*/', '|', preg_replace('/\s+/', ' ', $type));
    }

    private static function openBrackets(string $line): int
    {
        return preg_match_all('/[<({\[]/', $line) - preg_match_all('/(?<!-)[>)}\]]/', $line);
    }

    private function callsFuncNumArgs(ClassMethod $method): bool
    {
        foreach ($this->finder->findInstanceOf($method->stmts ?? [], Node\Expr\FuncCall::class) as $call) {
            if ($call->name instanceof Name && strtolower($call->name->toString()) === 'func_num_args') {
                return true;
            }
        }

        return false;
    }

    /**
     * The declaration whose signature a type exposes for a method: its own, then its class
     * ancestors', then its interfaces'.
     *
     */
    private function nearestDeclaration(string $fqcn, string $name): ClassMethod
    {
        for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
            $method = $this->ownMethods($type)[$name] ?? null;
            if ($method !== null) {
                return $method;
            }
        }
        foreach ($this->composerClosure($fqcn) as $type) {
            $method = $this->ownMethods($type)[$name] ?? null;
            if ($method !== null) {
                return $method;
            }
        }
        throw new \LogicException("$fqcn::$name has no declaration");
    }

    /**
     * The topmost Composer declaration of a method: the interface declaring it that no Composer
     * interface it extends declares it too, or, when no interface declares it, the topmost class.
     */
    private function topmostDeclaration(string $fqcn, string $name): string
    {
        $interfaces = [];
        $classes = [];
        foreach ($this->composerClosure($fqcn) as $type) {
            if (!isset($this->ownMethods($type)[$name])) {
                continue;
            }
            if ($this->composerSource($type)->classLike instanceof Interface_) {
                $interfaces[] = $type;
            } else {
                $classes[] = $type;
            }
        }
        if ($interfaces !== []) {
            $topmost = [];
            foreach ($interfaces as $interface) {
                $shadowed = false;
                foreach ($interfaces as $other) {
                    if ($other !== $interface && in_array($other, $this->composerClosure($interface), true)) {
                        $shadowed = true;
                        break;
                    }
                }
                if (!$shadowed) {
                    $topmost[] = $interface;
                }
            }
            if (count($topmost) > 1) {
                sort($topmost, SORT_STRING);
                $this->errors[] = "$fqcn::$name is declared by unrelated interfaces " . implode(', ', $topmost)
                    . '; its topmost declaration is ambiguous';
            }
            return $topmost[0];
        }
        $topmost = null;
        for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
            if (in_array($type, $classes, true)) {
                $topmost = $type;
            }
        }

        return $topmost;
    }

    /**
     * Whether a call on an instance of the class (or on the class, for a static method) reaches
     * the Rust side, which it does when the stub or runtime class, or an ancestor stub, forwards
     * it; and if so, whether the Rust side answers it (`status`, as `forced.json` records it:
     * `implemented`, `unimplemented`, or `refused` with its `reason`).
     *
     * @return array{forwarded: bool, status?: string, reason?: string}
     */
    private function forwarding(string $fqcn, string $name, string $kind): array
    {
        if ($kind === 'method' && $this->answersLocally($fqcn, $name)) {
            return ['forwarded' => false];
        }
        $types = $kind === 'method' ? [] : [$fqcn];
        if ($kind === 'method') {
            for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
                $types[] = $type;
            }
        }
        foreach ($types as $type) {
            foreach ($this->report->forwarded($type) as $member) {
                if ($member['kind'] === $kind && $member['member'] === $name) {
                    $row = [
                        'forwarded' => true,
                        'status' => $member['status'] === 'forward' ? 'implemented' : $member['status'],
                    ];
                    if ($member['status'] === 'refused') {
                        $row['reason'] = $member['reason'];
                    }
                    return $row;
                }
            }
        }

        return ['forwarded' => false];
    }

    private function answersLocally(string $fqcn, string $name): bool
    {
        for ($type = $fqcn; $type !== null; $type = $this->composerParent($type)) {
            if (isset(Generator::LOCAL_BODIES["$type::$name"])) {
                return true;
            }
        }

        return false;
    }

    /**
     * Every member the report says the class forwards is described.
     *
     * @param list<array<string, mixed>> $methods
     * @param ?array<string, mixed>      $constructor
     */
    private function checkForwardedCovered(string $fqcn, array $methods, ?array $constructor): void
    {
        $described = [];
        foreach ($methods as $method) {
            $described[$method['static'] ? 'static' : 'method'][$method['name']] = true;
        }
        if ($constructor !== null) {
            $described['constructor']['__construct'] = true;
        }
        foreach ($this->forwardedMembers($fqcn) as $member) {
            if (!isset($described[$member['kind']][$member['member']])) {
                $this->errors[] = "$fqcn forwards the {$member['kind']} {$member['member']}, which no Composer type declares";
            }
        }
    }

    /** @return list<array{member: string, kind: string, status: string, reason?: string}> */
    private function forwardedMembers(string $fqcn): array
    {
        return $this->report->forwarded($fqcn);
    }

    /**
     * The type and its Composer supertypes, each before the ones it extends or implements.
     *
     * @return list<string>
     */
    private function composerClosure(string $fqcn): array
    {
        $out = [];
        $queue = [$fqcn];
        while ($queue !== []) {
            $type = array_shift($queue);
            if (in_array($type, $out, true)) {
                continue;
            }
            $out[] = $type;
            foreach ($this->directSupertypes($type) as $supertype) {
                $queue[] = $supertype;
            }
        }

        return $out;
    }

    /** @return list<string> the parent and the interfaces of a type that are Composer types */
    private function directSupertypes(string $fqcn): array
    {
        $parent = $this->composerParent($fqcn);

        return array_merge($parent === null ? [] : [$parent], $this->composerInterfaces($fqcn));
    }

    private function composerParent(string $fqcn): ?string
    {
        $classLike = $this->composerSource($fqcn)->classLike;
        if (!$classLike instanceof Class_ || $classLike->extends === null) {
            return null;
        }
        $parent = SourceFile::resolvedName($classLike->extends);

        return $this->composerSource($parent) === null ? null : $parent;
    }

    /** @return list<string> the interfaces a type implements or extends directly, Composer ones only */
    private function composerInterfaces(string $fqcn): array
    {
        $classLike = $this->composerSource($fqcn)->classLike;
        $names = match (true) {
            $classLike instanceof Class_ => $classLike->implements,
            $classLike instanceof Interface_ => $classLike->extends,
            default => [],
        };
        $out = [];
        foreach ($names as $name) {
            $interface = SourceFile::resolvedName($name);
            if ($this->composerSource($interface) !== null) {
                $out[] = $interface;
            }
        }

        return $out;
    }

    /**
     * The methods a type declares itself, those of the traits it uses included.
     *
     * @return array<string, ClassMethod>
     */
    private function ownMethods(string $fqcn): array
    {
        if (isset($this->ownMethods[$fqcn])) {
            return $this->ownMethods[$fqcn];
        }
        $classLike = $this->composerSource($fqcn)->classLike;
        $methods = [];
        foreach ($classLike->getMethods() as $method) {
            $methods[$method->name->toString()] = $method;
        }
        foreach ($classLike->getTraitUses() as $use) {
            if ($use->adaptations !== []) {
                $this->errors[] = "$fqcn adapts the methods of a trait it uses, which is not described yet";
            }
            foreach ($use->traits as $trait) {
                $traitFqcn = SourceFile::resolvedName($trait);
                if ($this->composerSource($traitFqcn) === null) {
                    $this->errors[] = "$fqcn uses $traitFqcn, which is not a Composer trait";
                    continue;
                }
                foreach ($this->ownMethods($traitFqcn) as $name => $method) {
                    $methods[$name] ??= $method;
                }
            }
        }

        return $this->ownMethods[$fqcn] = $methods;
    }

    private function composerSource(string $fqcn): ?SourceFile
    {
        if (array_key_exists($fqcn, $this->composerSources)) {
            return $this->composerSources[$fqcn];
        }
        $builtin = (class_exists($fqcn, false) || interface_exists($fqcn, false))
            && (new \ReflectionClass($fqcn))->isInternal();
        $source = null;
        if (!$builtin) {
            $file = $this->project->sourceFor($fqcn);
            if (str_starts_with($file->path, $this->composerRoot . '/src/')) {
                $source = $file;
            }
        }

        return $this->composerSources[$fqcn] = $source;
    }
}
