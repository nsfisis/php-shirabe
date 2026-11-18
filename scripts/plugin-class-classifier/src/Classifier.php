<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Computes, from the Composer sources and inputs/, where the instances of each class live (its
 * value), what the worker puts at its FQCN (its treatment), and what the Rust side has to answer
 * for it. See docs/dev/plugin-class-classification.md.
 */
final class Classifier
{
    public const VALUES = ['rust-handle', 'rust-value', 'worker', 'refused'];

    public const TREATMENTS = ['stub', 'runtime', 'by-value', 'real', 'guard'];

    private const DEFAULT_TREATMENTS = [
        'rust-handle' => 'stub',
        'rust-value' => 'by-value',
        'worker' => 'real',
        'refused' => 'guard',
    ];

    private const THROWABLE_ROOTS = ['Throwable', 'Exception', 'Error'];

    public SourceParser $sources;

    public ReachabilityClosure $closure;

    public PurityAnalyzer $purity;

    /** @var array<string, array<string, mixed>> FQCN => row */
    public array $rows = [];

    /** @var list<string> */
    public array $violations = [];

    /** @var list<array<string, mixed>> */
    public array $backlog = [];

    /** @var array<string, bool> */
    private array $throwableCache = [];

    /** @var array<string, bool> */
    private array $valueObjectCandidates = [];

    /** @var array<string, bool> */
    private array $statelessPureCandidates = [];

    /** @var array<string, ?string> FQCN => the rule's value (null for interfaces and traits) */
    private array $ruleValues = [];

    /** @var array<string, list<string>> FQCN => why the rule gave its value */
    private array $ruleReasons = [];

    /** @var array<string, ?string> FQCN => the value after overrides */
    private array $values = [];

    /** @var array<string, string> FQCN => treatment */
    private array $treatments = [];

    /** @var array<string, string> FQCN => default | forced | propagated */
    private array $treatmentSources = [];

    /** @var array<string, list<array{member: string, kind: string, target: string}>> FQCN => the hardRefs that guarded it */
    private array $propagatedBy = [];

    /** @var array<string, array{kind: string, abstract: bool, origin: string}> vendor or builtin FQCN => declaration */
    private array $foreignClasses = [];

    /** @var array<string, list<array{member: string, kind: string, status: string, reason?: string}>> */
    private array $forwarded = [];

    /** @var array<string, array<string, list<int|string>>> stub/runtime FQCN => instance surface */
    private array $surfaces = [];

    /** @var array<string, bool> FQCN => whether `new FQCN` builds a Rust-side entity */
    private array $constructsEntity = [];

    public function __construct(
        private readonly string $composerSrc,
        private readonly Inputs $inputs,
    ) {
    }

    public function run(): void
    {
        $this->sources = new SourceParser();
        $this->sources->parseTree($this->composerSrc);
        $this->sources->finalize();

        $this->checkTwoWorld();

        $this->closure = new ReachabilityClosure($this->sources, $this->inputs);
        $this->closure->run();

        $this->purity = new PurityAnalyzer($this->sources);
        $this->purity->run();

        $this->computeLocalCandidates();

        foreach ($this->sources->classes as $fqcn => $info) {
            [$this->ruleValues[$fqcn], $this->ruleReasons[$fqcn]] = $this->ruleValue($fqcn, $info);
            $this->values[$fqcn] = $this->ruleValues[$fqcn];
        }
        $this->applyOverrides();

        foreach (array_keys($this->values) as $fqcn) {
            $this->treatments[$fqcn] = $this->defaultTreatment($fqcn);
            $this->treatmentSources[$fqcn] = 'default';
        }
        $this->applyForcedTreatments();
        [$this->treatments, $this->treatmentSources, $this->propagatedBy] = $this->propagate(null);
        $this->checkForcedMembers();
        $this->checkRedundantForcedReal();

        $this->computeForwarded();
        $this->checkForcedMemberKeys();

        foreach ($this->sources->classes as $fqcn => $info) {
            $this->rows[$fqcn] = $this->row($fqcn, $info);
        }
        foreach ($this->foreignClasses as $fqcn => $declaration) {
            $this->rows[$fqcn] = $this->foreignRow($fqcn, $declaration);
        }
        $this->checkStaticState();
        $this->checkForcedProperties();
        $this->computeBacklog();
        $this->collectViolations();
        ksort($this->rows);
    }

    public function value(string $fqcn): ?string
    {
        return $this->values[$fqcn] ?? null;
    }

    public function treatment(string $fqcn): ?string
    {
        return $this->treatments[$fqcn] ?? null;
    }

    public function foreignValue(string $fqcn): ?string
    {
        return isset($this->foreignClasses[$fqcn]) ? $this->values[$fqcn] : null;
    }

    /** @return array{?string, list<string>} */
    private function ruleValue(string $fqcn, ClassInfo $info): array
    {
        if ($info->kind === 'interface' || $info->kind === 'trait') {
            return [null, ['interfaces and traits have no instances of their own']];
        }
        if ($this->inputs->isTwoWorld($fqcn)) {
            return ['worker', ['two-world (two-world.json): the worker runs its own sibling implementation']];
        }
        if (!isset($this->closure->reachable[$fqcn])) {
            return ['worker', ['not plugin-reachable: no instance the Rust side owns ever reaches plugin code']];
        }
        if ($this->isThrowable($fqcn)) {
            return ['worker', ['exception class: thrown and caught where it is raised, crossing as a throwable record']];
        }
        if ($this->isConstantsOnly($info)) {
            return ['worker', ['constants-only class: pure definitions, no state to share']];
        }
        if ($this->statelessPureCandidates[$fqcn] ?? false) {
            return ['worker', ['stateless pure class: no instance state, all methods pure, real code answers identically on both sides']];
        }
        if ($this->valueObjectCandidates[$fqcn] ?? false) {
            return ['rust-value', ['value object: no state referencing living services, all methods pure']];
        }

        return ['rust-handle', ['reachable class holding or reaching shared state']];
    }

    private function defaultTreatment(string $fqcn): string
    {
        $value = $this->values[$fqcn];

        return $value === null ? 'real' : self::DEFAULT_TREATMENTS[$value];
    }

    private function applyOverrides(): void
    {
        foreach ($this->inputs->overrides as $fqcn => $override) {
            if (isset($this->sources->classes[$fqcn])) {
                $ruleValue = $this->ruleValues[$fqcn];
                if ($ruleValue === null) {
                    $this->violations[] = "overrides.json: $fqcn is an interface or a trait, which has no value";
                    continue;
                }
                if ($ruleValue === $override['value']) {
                    $this->violations[] = "overrides.json: $fqcn: the rules already give {$override['value']}";
                    continue;
                }
                $this->values[$fqcn] = $override['value'];
                continue;
            }
            $declaration = $this->foreignDeclaration($fqcn);
            if ($declaration === null) {
                $this->violations[] = "overrides.json: unknown class $fqcn";
                continue;
            }
            if ($declaration['kind'] === 'interface' || $declaration['kind'] === 'trait') {
                $this->violations[] = "overrides.json: $fqcn is an interface or a trait, which has no value";
                continue;
            }
            $packageValue = $declaration['origin'] === 'builtin' ? 'worker' : VendorPackages::lookup($fqcn)['value'];
            if ($packageValue === $override['value']) {
                $this->violations[] = "overrides.json: $fqcn: its package is already $packageValue";
                continue;
            }
            $this->foreignClasses[$fqcn] = $declaration;
            $this->values[$fqcn] = $override['value'];
        }
    }

    /**
     * A vendor class from composer/vendor or a PHP builtin class, read just far enough to tell
     * its kind.
     *
     * @return array{kind: string, abstract: bool, origin: string}|null
     */
    private function foreignDeclaration(string $fqcn): ?array
    {
        if (!str_contains($fqcn, '\\')) {
            if (!class_exists($fqcn, false) && !interface_exists($fqcn, false)) {
                return null;
            }
            $reflection = new \ReflectionClass($fqcn);
            if (!$reflection->isInternal()) {
                return null;
            }

            return [
                'kind' => $reflection->isInterface() ? 'interface' : ($reflection->isTrait() ? 'trait' : ($reflection->isEnum() ? 'enum' : 'class')),
                'abstract' => $reflection->isAbstract() && !$reflection->isInterface(),
                'origin' => 'builtin',
            ];
        }
        if (VendorPackages::lookup($fqcn) === null) {
            return null;
        }
        $file = $this->vendorFile($fqcn);
        if ($file === null) {
            return null;
        }
        $short = substr($fqcn, strrpos($fqcn, '\\') + 1);
        if (preg_match('/^\s*((?:(?:abstract|final|readonly)\s+)*)(class|interface|trait|enum)\s+' . preg_quote($short, '/') . '\b/m', (string) file_get_contents($file), $m) !== 1) {
            return null;
        }

        return ['kind' => $m[2], 'abstract' => str_contains($m[1], 'abstract'), 'origin' => 'vendor'];
    }

    private function vendorFile(string $fqcn): ?string
    {
        $vendorDir = dirname($this->composerSrc, 2) . '/vendor';
        $map = @include $vendorDir . '/composer/autoload_psr4.php';
        if (!is_array($map)) {
            return null;
        }
        foreach ($map as $prefix => $dirs) {
            if (!str_starts_with($fqcn, $prefix)) {
                continue;
            }
            foreach ($dirs as $dir) {
                $path = $dir . '/' . str_replace('\\', '/', substr($fqcn, strlen($prefix))) . '.php';
                if (is_file($path)) {
                    return $path;
                }
            }
        }

        return null;
    }

    private function applyForcedTreatments(): void
    {
        foreach ($this->inputs->forced as $fqcn => $entry) {
            if (!array_key_exists($fqcn, $this->values)) {
                $this->violations[] = "forced.json: unknown class $fqcn";
                continue;
            }
            if (!isset($entry['treatment'])) {
                continue;
            }
            $treatment = $entry['treatment'];
            $kind = $this->kindOf($fqcn);
            if ($kind === 'interface' || $kind === 'trait') {
                $this->violations[] = "forced.json: $fqcn is a $kind, whose treatment is always real";
                continue;
            }
            $needsReason = in_array($treatment, ['real', 'runtime'], true);
            if ($needsReason !== isset($entry['reason'])) {
                $this->violations[] = $needsReason
                    ? "forced.json: $fqcn: a $treatment treatment needs a reason"
                    : "forced.json: $fqcn: a $treatment treatment takes no reason";
            }
            if ($treatment === $this->treatments[$fqcn] && $treatment !== 'real') {
                $this->violations[] = "forced.json: $fqcn: $treatment is already its default treatment";
                continue;
            }
            $this->treatments[$fqcn] = $treatment;
            $this->treatmentSources[$fqcn] = 'forced';
        }
    }

    /**
     * A forced `real` that equals the default treatment is only meaningful when it keeps the guard
     * propagation away from the class.
     */
    private function checkRedundantForcedReal(): void
    {
        foreach ($this->inputs->forced as $fqcn => $entry) {
            if (($entry['treatment'] ?? null) !== 'real' || $this->defaultTreatment($fqcn) !== 'real') {
                continue;
            }
            [$treatments] = $this->propagate($fqcn);
            if ($treatments[$fqcn] !== 'guard') {
                $this->violations[] = "forced.json: $fqcn: real is already its treatment";
            }
        }
    }

    /**
     * The guard propagation: a worker class whose real code would construct an entity the Rust
     * side cannot build, or reach a class this rule guards, is guarded itself.
     *
     * @param ?string $unforced a class whose forced treatment is ignored
     * @return array{array<string, string>, array<string, string>, array<string, list<array{member: string, kind: string, target: string}>>}
     */
    private function propagate(?string $unforced): array
    {
        $treatments = $this->treatments;
        $sources = $this->treatmentSources;
        if ($unforced !== null) {
            $treatments[$unforced] = $this->defaultTreatment($unforced);
            $sources[$unforced] = 'default';
        }
        $by = [];
        do {
            $changed = false;
            foreach ($this->sources->classes as $fqcn => $info) {
                if ($info->kind !== 'class' || $sources[$fqcn] !== 'default' || $this->values[$fqcn] !== 'worker'
                    || $this->inputs->isTwoWorld($fqcn)) {
                    continue;
                }
                $origins = [];
                foreach ($info->hardRefs as $ref) {
                    if ($ref['target'] === $fqcn) {
                        continue;
                    }
                    if ($ref['kind'] === 'new') {
                        $target = $ref['target'];
                        if (($treatments[$target] ?? null) === 'guard'
                            || (($treatments[$target] ?? null) === 'stub' && $this->constructorRefused($target))) {
                            $origins[] = $ref;
                        }
                    } else {
                        $target = explode('::', $ref['target'])[0];
                        if (($sources[$target] ?? null) === 'propagated') {
                            $origins[] = $ref;
                        }
                    }
                }
                if ($origins !== []) {
                    $origins = array_values(array_unique($origins, SORT_REGULAR));
                    $treatments[$fqcn] = 'guard';
                    $sources[$fqcn] = 'propagated';
                    $by[$fqcn] = $origins;
                    $changed = true;
                }
            }
        } while ($changed);

        return [$treatments, $sources, $by];
    }

    /** Whether the stub's constructor answers with an error rather than building an entity. */
    private function constructorRefused(string $fqcn): bool
    {
        $status = $this->inputs->forced[$fqcn]['members']['__construct']['status'] ?? null;

        return $status === 'unimplemented' || $status === 'refused';
    }

    private function checkForcedMembers(): void
    {
        foreach ($this->inputs->forced as $fqcn => $entry) {
            if (!array_key_exists($fqcn, $this->treatments)) {
                continue;
            }
            $treatment = $this->treatments[$fqcn];
            if (isset($entry['members']) && !in_array($treatment, ['stub', 'runtime'], true)) {
                $this->violations[] = "forced.json: $fqcn: members belong to a stub or runtime class, not a $treatment one";
            }
            if (isset($entry['statics']) && $treatment !== 'guard') {
                $this->violations[] = "forced.json: $fqcn: statics belong to a guard class, not a $treatment one";
            }
            if (isset($entry['properties']) && $treatment !== 'runtime') {
                $this->violations[] = "forced.json: $fqcn: properties belong to a runtime class, not a $treatment one";
            }
        }
    }

    /**
     * What the worker-side definition of each stub and runtime class forwards to the Rust side,
     * per declaring class, and the static methods each guard forwards.
     */
    private function computeForwarded(): void
    {
        $classes = array_keys($this->sources->classes);
        sort($classes);
        foreach ($classes as $fqcn) {
            $treatment = $this->treatments[$fqcn];
            if (($treatment === 'stub' || $treatment === 'runtime') && $this->values[$fqcn] === 'rust-handle') {
                $this->forwarded[$fqcn] = $this->forwardedBy($fqcn);
            } elseif ($treatment === 'guard' && isset($this->inputs->forced[$fqcn]['statics'])) {
                $rows = [];
                foreach ($this->inputs->forced[$fqcn]['statics'] as $name => $static) {
                    $method = $this->sources->classes[$fqcn]->methods[strtolower($name)] ?? null;
                    if ($method === null || !$method->static || $method->visibility !== 'public') {
                        $this->violations[] = "forced.json: $fqcn::$name is not a public static method";
                        continue;
                    }
                    $rows[] = ['member' => $method->name, 'kind' => 'static', 'status' => 'forward', 'reason' => $static['reason']];
                }
                $this->forwarded[$fqcn] = $rows;
            }
        }
    }

    /** @return list<array{member: string, kind: string, status: string, reason?: string}> */
    private function forwardedBy(string $fqcn): array
    {
        $info = $this->sources->classes[$fqcn];
        $rows = [];
        foreach (array_keys($this->emittedMethods($fqcn)) as $name) {
            $rows[] = $this->forwardedRow($fqcn, $name, 'method');
        }
        if ($this->treatments[$fqcn] === 'stub') {
            if (!$info->abstract && $this->constructsEntity($fqcn)) {
                $rows[] = $this->forwardedRow($fqcn, '__construct', 'constructor');
            }
            foreach ($this->forwardedStatics($fqcn) as $name) {
                $rows[] = $this->forwardedRow($fqcn, $name, 'static');
            }
        }
        usort($rows, static fn (array $a, array $b) => strcmp($a['member'], $b['member']));

        return $rows;
    }

    /** @return array{member: string, kind: string, status: string, reason?: string} */
    private function forwardedRow(string $fqcn, string $member, string $kind): array
    {
        $override = $this->inputs->forced[$fqcn]['members'][$member] ?? null;
        $row = ['member' => $member, 'kind' => $kind, 'status' => $override['status'] ?? 'implemented'];
        if (isset($override['reason'])) {
            $row['reason'] = $override['reason'];
        }

        return $row;
    }

    /**
     * The instance methods the worker-side definition of a stub or runtime class declares as
     * forwarders: a class whose parent forwards nothing declares its whole surface (its
     * interfaces' methods and its own public methods), any other one the methods its parent's
     * surface lacks.
     *
     * @return array<string, true>
     */
    private function emittedMethods(string $fqcn): array
    {
        $info = $this->sources->classes[$fqcn];
        $parent = $info->parent;
        $parentSurface = $parent !== null && in_array($this->treatments[$parent] ?? null, ['stub', 'runtime'], true)
            ? $this->surfaceOf($parent)
            : [];
        $emitted = [];
        foreach ($this->interfaceClosure($info) as $interface) {
            foreach ($interface->methods as $method) {
                if (!$method->static && !isset($parentSurface[$method->name])) {
                    $emitted[$method->name] ??= true;
                }
            }
        }
        foreach ($this->ownPublicInstanceMethods($info) as $name) {
            if (!isset($parentSurface[$name])) {
                $emitted[$name] = true;
            }
        }
        ksort($emitted);

        return $emitted;
    }

    /** @return array<string, true> every instance method the worker-side definition answers */
    private function surfaceOf(string $fqcn): array
    {
        if (isset($this->surfaces[$fqcn])) {
            return $this->surfaces[$fqcn];
        }
        $info = $this->sources->classes[$fqcn];
        $surface = [];
        if ($info->parent !== null && isset($this->sources->classes[$info->parent])) {
            $surface = $this->surfaceOf($info->parent);
        }
        foreach ($this->interfaceClosure($info) as $interface) {
            foreach ($interface->methods as $method) {
                if (!$method->static) {
                    $surface[$method->name] = true;
                }
            }
        }
        foreach ($this->ownPublicInstanceMethods($info) as $name) {
            $surface[$name] = true;
        }

        return $this->surfaces[$fqcn] = $surface;
    }

    /** @return list<string> */
    private function ownPublicInstanceMethods(ClassInfo $info): array
    {
        $names = [];
        foreach ($info->methods as $lname => $method) {
            if (isset($info->traitMethods[$lname]) || $method->static || $method->visibility !== 'public') {
                continue;
            }
            if (str_starts_with($method->name, '__') && $method->name !== '__toString') {
                continue;
            }
            $names[] = $method->name;
        }

        return $names;
    }

    /** @return list<ClassInfo> the interfaces a class implements, each before the ones it extends */
    private function interfaceClosure(ClassInfo $info): array
    {
        $out = [];
        $seen = [];
        $visit = function (string $fqcn) use (&$visit, &$out, &$seen): void {
            if (isset($seen[$fqcn])) {
                return;
            }
            $seen[$fqcn] = true;
            $interface = $this->sources->classes[$fqcn] ?? null;
            if ($interface === null) {
                return;
            }
            $out[] = $interface;
            foreach ($interface->interfaces as $parent) {
                $visit($parent);
            }
        };
        foreach ($info->interfaces as $interface) {
            $visit($interface);
        }

        return $out;
    }

    /**
     * Whether `new FQCN` on the worker side reaches the Rust side: a stub declares a forwarding
     * constructor when its parent forwards nothing or when it declares a public one itself, and
     * inherits its parent's otherwise; a runtime class constructs its own instance.
     */
    private function constructsEntity(string $fqcn): bool
    {
        if (isset($this->constructsEntity[$fqcn])) {
            return $this->constructsEntity[$fqcn];
        }
        $info = $this->sources->classes[$fqcn];
        $treatment = $this->treatments[$fqcn];
        if ($treatment !== 'stub') {
            return $this->constructsEntity[$fqcn] = false;
        }
        $parent = $info->parent;
        $parentForwards = $parent !== null && in_array($this->treatments[$parent] ?? null, ['stub', 'runtime'], true);
        $ctor = $info->methods['__construct'] ?? null;
        if (!$parentForwards || ($ctor !== null && !isset($info->traitMethods['__construct']) && $ctor->visibility === 'public')) {
            return $this->constructsEntity[$fqcn] = true;
        }

        return $this->constructsEntity[$fqcn] = $this->constructsEntity($parent);
    }

    /**
     * The public static methods of a stub that forward instead of running as copies of the real
     * code in the worker: the ones reaching a static property, whose value the Rust side owns, or
     * a class a guard shadows there, directly or through the non-public static helpers they call.
     *
     * @return list<string>
     */
    private function forwardedStatics(string $fqcn): array
    {
        $info = $this->sources->classes[$fqcn];
        $helpers = [];
        foreach ($info->methods as $lname => $method) {
            if ($method->static && $method->visibility !== 'public' && !isset($info->traitMethods[$lname])) {
                $helpers[$lname] = $method;
            }
        }
        $forwarded = [];
        foreach ($info->methods as $lname => $method) {
            if (!$method->static || $method->visibility !== 'public' || isset($info->traitMethods[$lname])
                || str_starts_with($method->name, '__')) {
                continue;
            }
            $queue = [$method];
            $seen = [];
            while ($queue !== []) {
                $current = array_shift($queue);
                if ($current->touchesStaticProps) {
                    $forwarded[] = $method->name;
                    continue 2;
                }
                foreach ($current->classRefs as $ref) {
                    if (($this->treatments[$ref] ?? null) === 'guard') {
                        $forwarded[] = $method->name;
                        continue 3;
                    }
                }
                foreach ($current->ownStaticCalls as $called) {
                    if (isset($helpers[$called]) && !isset($seen[$called])) {
                        $seen[$called] = true;
                        $queue[] = $helpers[$called];
                    }
                }
            }
        }

        return $forwarded;
    }

    private function checkForcedMemberKeys(): void
    {
        foreach ($this->inputs->forced as $fqcn => $entry) {
            if (!isset($entry['members'])) {
                continue;
            }
            $known = [];
            foreach ($this->forwarded[$fqcn] ?? [] as $row) {
                $known[$row['member']] = true;
            }
            foreach (array_keys($entry['members']) as $member) {
                if (!isset($known[$member])) {
                    $this->violations[] = "forced.json: $fqcn does not forward $member";
                }
            }
        }
    }

    /** @return array<string, mixed> */
    private function row(string $fqcn, ClassInfo $info): array
    {
        $reachableBits = $this->closure->reachable[$fqcn] ?? 0;
        $row = [
            'fqcn' => $fqcn,
            'kind' => $info->kind,
            'abstract' => $info->abstract,
            'reachable' => $reachableBits !== 0,
            'direction' => $this->directionLabel($reachableBits),
            'value' => $this->values[$fqcn],
        ];
        if (isset($this->inputs->overrides[$fqcn]) && $this->values[$fqcn] !== $this->ruleValues[$fqcn]) {
            $row['ruleValue'] = $this->ruleValues[$fqcn];
        }
        $row['treatment'] = $this->treatments[$fqcn];
        $row['treatmentSource'] = $this->treatmentSources[$fqcn];

        $reasons = $this->ruleReasons[$fqcn];
        if (isset($row['ruleValue'])) {
            $reasons[] = 'override: ' . $this->inputs->overrides[$fqcn]['reason'];
        }
        if ($this->treatmentSources[$fqcn] === 'forced') {
            $reasons[] = 'forced: ' . ($this->inputs->forced[$fqcn]['reason'] ?? 'no stub or codec yet');
        } elseif ($this->treatmentSources[$fqcn] === 'propagated') {
            foreach ($this->propagatedBy[$fqcn] as $ref) {
                $reasons[] = $ref['kind'] === 'new'
                    ? "propagated: {$ref['member']}() constructs {$ref['target']}"
                    : "propagated: {$ref['member']}() calls {$ref['target']}";
            }
        }
        $row['reasons'] = $reasons;

        $hardRefs = [];
        if ($info->parent !== null) {
            $hardRefs[] = ['member' => null, 'kind' => 'extends', 'target' => $info->parent];
        }
        $seen = [];
        foreach ($info->hardRefs as $ref) {
            $key = "{$ref['member']}\0{$ref['kind']}\0{$ref['target']}";
            if (!isset($seen[$key])) {
                $seen[$key] = true;
                $hardRefs[] = $ref;
            }
        }
        $row['hardRefs'] = $hardRefs;

        $attributes = [];
        $dispositions = $this->dispositions($fqcn, $info);
        if ($dispositions !== []) {
            $attributes['mutable-static'] = $dispositions;
        }
        if ($this->isThrowable($fqcn)) {
            $attributes['throwable'] = true;
        }
        if ($info->kind === 'class') {
            $attributes['plugin-constructible'] = $this->hasPublicConstructor($info);
        }
        $statics = [];
        foreach ($info->properties as $prop) {
            if ($prop->static && $prop->visibility === 'public') {
                $statics[] = $prop->name;
            }
        }
        if ($statics !== []) {
            sort($statics);
            $attributes['public-static-properties'] = $statics;
        }
        $row['attributes'] = $attributes;
        $row['methods'] = $this->methodRows($fqcn, $info);

        if (isset($this->forwarded[$fqcn])) {
            $row['forwarded'] = $this->forwarded[$fqcn];
        }
        $staticProperties = $this->staticPropertyRows($fqcn, $dispositions);
        if ($staticProperties !== []) {
            $row['staticProperties'] = $staticProperties;
        }

        return $row;
    }

    /**
     * @param array{kind: string, abstract: bool, origin: string} $declaration
     * @return array<string, mixed>
     */
    private function foreignRow(string $fqcn, array $declaration): array
    {
        $bits = $declaration['origin'] === 'vendor'
            ? ($this->closure->vendorReachable[$fqcn] ?? 0)
            : ($this->closure->builtinReachable[$fqcn] ?? 0);

        return [
            'fqcn' => $fqcn,
            'kind' => $declaration['kind'],
            'abstract' => $declaration['abstract'],
            'reachable' => $bits !== 0,
            'direction' => $this->directionLabel($bits),
            'value' => $this->values[$fqcn],
            'treatment' => $this->treatments[$fqcn],
            'treatmentSource' => $this->treatmentSources[$fqcn],
            'reasons' => [
                ($declaration['origin'] === 'vendor' ? 'vendor class' : 'PHP builtin class') . ', named by an override',
                'override: ' . $this->inputs->overrides[$fqcn]['reason'],
            ],
            'hardRefs' => [],
            'attributes' => [],
            'methods' => [],
        ];
    }

    /** @return array<string, string> property => disposition, for the static properties the class writes */
    private function dispositions(string $fqcn, ClassInfo $info): array
    {
        $out = [];
        foreach (array_unique($info->staticWrites) as $property) {
            $out[$property] = $this->inputs->staticState[$fqcn][$property] ?? 'needs-review';
        }
        ksort($out);

        return $out;
    }

    /**
     * @param array<string, string> $dispositions
     * @return list<array{name: string, disposition: string, sync: ?string}>
     */
    private function staticPropertyRows(string $fqcn, array $dispositions): array
    {
        $rows = [];
        foreach ($dispositions as $property => $disposition) {
            $rows[] = [
                'name' => $property,
                'disposition' => $disposition,
                'sync' => $disposition === 'needs-sync' ? $this->syncOf($fqcn, $property) : null,
            ];
        }

        return $rows;
    }

    /**
     * How a shared static property is kept in sync: a stub forwards its accessors, the Rust side
     * pushes the state of a class whose real code runs in the worker, and nothing keeps a public
     * static of a stub or any static of a guard in sync.
     */
    private function syncOf(string $fqcn, string $property): string
    {
        if (isset($this->inputs->forced[$fqcn]['staticProperties'][$property])) {
            return 'unsynced';
        }

        return match ($this->treatments[$fqcn]) {
            'stub' => $this->isPublicStatic($fqcn, $property) ? 'unsynced' : 'forward',
            'real', 'runtime' => 'push',
            default => 'unsynced',
        };
    }

    private function isPublicStatic(string $fqcn, string $property): bool
    {
        foreach ($this->sources->classes[$fqcn]->properties as $prop) {
            if ($prop->static && $prop->name === $property) {
                return $prop->visibility === 'public';
            }
        }

        return false;
    }

    private function checkStaticState(): void
    {
        foreach ($this->inputs->staticState as $fqcn => $properties) {
            $info = $this->sources->classes[$fqcn] ?? null;
            if ($info === null) {
                $this->violations[] = "static-state.json: unknown class $fqcn";
                continue;
            }
            $written = array_flip($info->staticWrites);
            foreach (array_keys($properties) as $property) {
                if (!isset($written[$property])) {
                    $this->violations[] = "static-state.json: $fqcn does not write a static property \$$property";
                }
            }
        }
        foreach ($this->inputs->forced as $fqcn => $entry) {
            foreach (array_keys($entry['staticProperties'] ?? []) as $property) {
                if (($this->inputs->staticState[$fqcn][$property] ?? null) !== 'needs-sync') {
                    $this->violations[] = "forced.json: $fqcn::\$$property is not a needs-sync static property";
                }
            }
        }
        foreach ($this->rows as $fqcn => $row) {
            foreach ($row['staticProperties'] ?? [] as $property) {
                if ($property['sync'] !== 'unsynced' || isset($this->inputs->forced[$fqcn]['staticProperties'][$property['name']])) {
                    continue;
                }
                $this->violations[] = "$fqcn::\${$property['name']} is shared state with no way to sync it; record it in forced.json as unsynced";
            }
        }
    }

    private function checkForcedProperties(): void
    {
        foreach ($this->inputs->forced as $fqcn => $entry) {
            if (($this->treatments[$fqcn] ?? null) !== 'runtime' || !isset($this->sources->classes[$fqcn])) {
                continue;
            }
            $declared = [];
            foreach ($this->sources->classes[$fqcn]->properties as $prop) {
                $declared[$prop->name] = true;
            }
            if (!isset($entry['properties'])) {
                continue;
            }
            $listed = $entry['properties'];
            foreach (array_keys($declared) as $name) {
                if (!isset($listed[$name])) {
                    $this->violations[] = "forced.json: $fqcn::\$$name has no handling in properties";
                }
            }
            foreach (array_keys($listed) as $name) {
                if (!isset($declared[$name])) {
                    $this->violations[] = "forced.json: $fqcn does not declare \$$name";
                }
            }
        }
    }

    private function computeBacklog(): void
    {
        foreach ($this->rows as $fqcn => $row) {
            if ($row['treatment'] === 'guard' && in_array($row['value'], ['rust-handle', 'rust-value'], true)) {
                $this->backlog[] = [
                    'item' => 1,
                    'fqcn' => $fqcn,
                    'value' => $row['value'],
                    'reachable' => $row['reachable'],
                    'direction' => $row['direction'],
                    'plugin-constructible' => $row['attributes']['plugin-constructible'] ?? false,
                ];
            }
        }

        foreach ($this->rows as $fqcn => $row) {
            if ($row['treatment'] !== 'real' || !isset($this->sources->classes[$fqcn])) {
                continue;
            }
            if ($this->inputs->isTwoWorld($fqcn) && !$row['reachable']) {
                continue;
            }
            foreach ($row['hardRefs'] as $ref) {
                $problem = $this->unresolvable($ref);
                if ($problem !== null) {
                    $this->backlog[] = ['item' => 2, 'fqcn' => $fqcn] + $ref + ['problem' => $problem];
                }
            }
        }

        foreach ($this->rows as $fqcn => $row) {
            if ($row['treatmentSource'] === 'propagated') {
                $this->backlog[] = ['item' => 3, 'fqcn' => $fqcn, 'origins' => $this->propagatedBy[$fqcn]];
            }
        }

        foreach ($this->forwarded as $fqcn => $rows) {
            $members = [];
            foreach ($rows as $member) {
                if (!in_array($member['status'], ['unimplemented', 'refused'], true)) {
                    continue;
                }
                $entry = ['member' => $member['member'], 'kind' => $member['kind'], 'status' => $member['status']];
                if ($member['kind'] === 'constructor') {
                    $entry['plugin-constructible'] = $this->rows[$fqcn]['attributes']['plugin-constructible'] ?? false;
                }
                $members[] = $entry;
            }
            if ($members !== []) {
                $this->backlog[] = ['item' => 4, 'fqcn' => $fqcn, 'count' => count($members), 'members' => $members];
            }
        }

        foreach ($this->rows as $fqcn => $row) {
            if ($row['treatment'] === 'real' && $row['value'] === 'rust-handle' && !$row['abstract']) {
                $this->backlog[] = ['item' => 5, 'fqcn' => $fqcn];
            }
        }

        foreach ($this->rows as $fqcn => $row) {
            foreach ($row['staticProperties'] ?? [] as $property) {
                if ($property['sync'] === 'unsynced') {
                    $this->backlog[] = ['item' => 6, 'fqcn' => $fqcn, 'property' => $property['name']];
                }
            }
        }
    }

    /**
     * Why upstream code running in the worker cannot follow a hard reference, or null when it can.
     *
     * @param array{member: ?string, kind: string, target: string} $ref
     */
    private function unresolvable(array $ref): ?string
    {
        if ($ref['kind'] === 'static') {
            [$class, $member] = explode('::', $ref['target'], 2);
            if (($this->treatments[$class] ?? null) !== 'guard') {
                return null;
            }
            foreach ($this->forwarded[$class] ?? [] as $row) {
                if ($row['member'] === $member && $row['status'] === 'forward') {
                    return null;
                }
            }

            return "$class is guarded";
        }
        $class = $ref['target'];
        $treatment = $this->treatments[$class] ?? null;
        if ($treatment === 'guard') {
            return "$class is guarded";
        }
        if ($ref['kind'] === 'new' && $treatment === 'stub' && $this->constructorRefused($class)) {
            return "the constructor of the $class stub is " . $this->inputs->forced[$class]['members']['__construct']['status'];
        }

        return null;
    }

    private function kindOf(string $fqcn): ?string
    {
        return $this->sources->classes[$fqcn]->kind ?? $this->foreignClasses[$fqcn]['kind'] ?? null;
    }

    /** @return list<array<string, mixed>> */
    private function methodRows(string $fqcn, ClassInfo $info): array
    {
        $rows = [];
        foreach ($info->methods as $lname => $method) {
            if ($method->visibility === 'private') {
                continue;
            }
            $callableParams = [];
            foreach ($method->params as $i => $param) {
                if ($param->callable) {
                    $callableParams[] = $i;
                }
            }
            $rows[] = [
                'name' => $method->name,
                'visibility' => $method->visibility,
                'static' => $method->static,
                'purity' => $this->purity->verdicts["$fqcn::$lname"] ?? 'n/a',
                'byRefParams' => $method->byRefParamPositions(),
                'callableParams' => $callableParams,
            ];
        }
        usort($rows, static fn (array $a, array $b) => strcmp($a['name'], $b['name']));

        return $rows;
    }

    private function directionLabel(int $bits): ?string
    {
        return match ($bits) {
            0 => null,
            ReachabilityClosure::PROVIDED => 'provided',
            ReachabilityClosure::CONSUMED => 'consumed',
            default => 'both',
        };
    }

    private function isThrowable(string $fqcn): bool
    {
        if (isset($this->throwableCache[$fqcn])) {
            return $this->throwableCache[$fqcn];
        }
        // Pre-set to break inheritance cycles (malformed input).
        $this->throwableCache[$fqcn] = false;

        $info = $this->sources->classes[$fqcn] ?? null;
        if ($info === null) {
            $isGlobal = !str_contains($fqcn, '\\');
            $result = $isGlobal && (
                in_array($fqcn, self::THROWABLE_ROOTS, true)
                || str_ends_with($fqcn, 'Exception')
                || str_ends_with($fqcn, 'Error')
            );

            return $this->throwableCache[$fqcn] = $result;
        }

        foreach (array_merge($info->parent !== null ? [$info->parent] : [], $info->interfaces) as $ancestor) {
            if ($this->isThrowable($ancestor)) {
                return $this->throwableCache[$fqcn] = true;
            }
        }

        return false;
    }

    private function hasPublicConstructor(ClassInfo $info): bool
    {
        if ($info->abstract) {
            return false;
        }
        $current = $info;
        while (true) {
            $ctor = $current->methods['__construct'] ?? null;
            if ($ctor !== null) {
                return $ctor->visibility === 'public';
            }
            if ($current->parent === null || !isset($this->sources->classes[$current->parent])) {
                // No declared constructor anywhere visible: implicit public.
                return true;
            }
            $current = $this->sources->classes[$current->parent];
        }
    }

    private function computeLocalCandidates(): void
    {
        // Grow-only fixed point over two candidate kinds. Both require
        // every instance method in the hierarchy to be pure and the body
        // references to be locally satisfiable; they differ on state:
        //
        //   - value objects carry copyable state (>= 1 instance property)
        //     none of which references a living service -> rust-value
        //   - stateless pure classes carry no instance state at all;
        //     the real code answers identically on both sides -> worker
        do {
            $changed = false;
            foreach ($this->closure->reachable as $fqcn => $bits) {
                if ($this->isCandidate($fqcn)) {
                    continue;
                }
                $info = $this->sources->classes[$fqcn] ?? null;
                if ($info === null || $info->kind !== 'class') {
                    continue;
                }
                if ($this->inputs->isTwoWorld($fqcn) || $this->isThrowable($fqcn) || $this->isConstantsOnly($info)) {
                    continue;
                }
                if (!$this->allInstanceMethodsPure($fqcn) || !$this->bodyRefsAreLocal($info)) {
                    continue;
                }
                if ($this->hasInstanceProperties($info)) {
                    if ($this->stateIsValueOnly($info)) {
                        $this->valueObjectCandidates[$fqcn] = true;
                        $changed = true;
                    }
                } else {
                    $this->statelessPureCandidates[$fqcn] = true;
                    $changed = true;
                }
            }
        } while ($changed);
    }

    private function hasInstanceProperties(ClassInfo $info): bool
    {
        foreach ($this->hierarchyOf($info) as $level) {
            foreach ($level->properties as $prop) {
                if (!$prop->static) {
                    return true;
                }
            }
        }

        return false;
    }

    /**
     * Value objects and stateless pure classes run their real method bodies on both sides, so
     * what the bodies construct must be locally constructible: another candidate, an exception,
     * a constants-only or two-world class, or anything outside the reachable set.
     */
    private function bodyRefsAreLocal(ClassInfo $info): bool
    {
        foreach (array_unique($info->newRefs) as $ref) {
            if ($ref === $info->fqcn || $this->isCandidate($ref) || $this->isThrowable($ref)) {
                continue;
            }
            if (isset($this->closure->reachable[$ref])) {
                $target = $this->sources->classes[$ref] ?? null;
                if ($target !== null && ($this->isConstantsOnly($target) || $this->inputs->isTwoWorld($ref))) {
                    continue;
                }

                return false;
            }
        }

        return true;
    }

    private function isCandidate(string $fqcn): bool
    {
        return ($this->valueObjectCandidates[$fqcn] ?? false) || ($this->statelessPureCandidates[$fqcn] ?? false);
    }

    private function stateIsValueOnly(ClassInfo $info): bool
    {
        $types = [];
        // State includes inherited properties: a VcsDownloader subclass
        // carries its parent's ProcessExecutor even with no own fields.
        foreach ($this->hierarchyOf($info) as $level) {
            foreach ($level->properties as $prop) {
                $types = array_merge($types, $prop->classTypes);
                if ($prop->expandable || $prop->classTypes === []) {
                    $types = array_merge($types, $prop->docblockTypes);
                }
            }
        }
        $ctor = $info->methods['__construct'] ?? null;
        if ($ctor !== null) {
            foreach ($ctor->params as $param) {
                $types = array_merge($types, $param->classTypes);
                if ($param->expandable || $param->classTypes === []) {
                    $types = array_merge($types, $param->docblockTypes);
                }
            }
        }

        foreach (array_unique($types) as $type) {
            if ($type === $info->fqcn) {
                continue;
            }
            if (!isset($this->closure->reachable[$type]) && !isset($this->sources->classes[$type])) {
                $vendor = VendorPackages::lookup($type);
                if ($vendor !== null && !$vendor['valueSafe']) {
                    return false;
                }
                continue;
            }
            if ($this->isThrowable($type)) {
                continue;
            }
            if (!$this->isCandidate($type)) {
                return false;
            }
        }

        return true;
    }

    /** @return list<ClassInfo> the class and its parents visible in the sources */
    private function hierarchyOf(ClassInfo $info): array
    {
        $levels = [];
        $seen = [];
        $current = $info;
        while (true) {
            if (isset($seen[$current->fqcn])) {
                break;
            }
            $seen[$current->fqcn] = true;
            $levels[] = $current;
            if ($current->parent === null || !isset($this->sources->classes[$current->parent])) {
                break;
            }
            $current = $this->sources->classes[$current->parent];
        }

        return $levels;
    }

    private function allInstanceMethodsPure(string $fqcn): bool
    {
        $info = $this->sources->classes[$fqcn];
        foreach ($this->hierarchyOf($info) as $level) {
            foreach ($level->methods as $lname => $method) {
                if ($method->static || $method->visibility === 'private' || $lname === '__construct') {
                    continue;
                }
                $verdict = $this->purity->verdicts["{$level->fqcn}::$lname"] ?? 'mutator';
                if ($verdict === 'mutator') {
                    return false;
                }
            }
        }

        return true;
    }

    private function isConstantsOnly(ClassInfo $info): bool
    {
        return $info->methods === [] && $info->properties === []
            && $info->parent === null && $info->interfaces === [];
    }

    private function checkTwoWorld(): void
    {
        $prefixes = array_keys($this->inputs->twoWorld);
        foreach ($prefixes as $prefix) {
            foreach ($prefixes as $other) {
                if ($other !== $prefix && str_starts_with($prefix, rtrim($other, '\\') . '\\')) {
                    $this->violations[] = "two-world.json: $prefix is already covered by $other";
                }
            }
            $matches = false;
            foreach (array_keys($this->sources->classes) as $fqcn) {
                if ($fqcn === $prefix || str_starts_with($fqcn, rtrim($prefix, '\\') . '\\')) {
                    $matches = true;
                    break;
                }
            }
            if (!$matches && !in_array($prefix, array_map(static fn (string $p) => rtrim($p, '\\'), array_keys(VendorPackages::PREFIXES)), true)) {
                $this->violations[] = "two-world.json: $prefix names no Composer class or vendor package";
            }
        }
    }

    private function collectViolations(): void
    {
        foreach ($this->rows as $fqcn => $row) {
            foreach ($row['attributes']['mutable-static'] ?? [] as $property => $disposition) {
                if ($disposition === 'needs-review') {
                    $this->violations[] = "$fqcn writes the static property \$$property but has no disposition in static-state.json";
                }
            }
        }
        foreach ($this->closure->unknownReachable as $fqcn => $bits) {
            $this->violations[] = "reachable type $fqcn is neither a composer class, a known vendor package, nor a PHP builtin";
        }
    }
}
