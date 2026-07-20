<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class Classifier
{
    public const CATEGORIES = [
        'rust-proxy', 'rust-snapshot', 'contract', 'two-world', 'php-native', 'unsupported',
    ];

    private const THROWABLE_ROOTS = ['Throwable', 'Exception', 'Error'];

    public SourceParser $sources;

    public ReachabilityClosure $closure;

    public PurityAnalyzer $purity;

    public NativeFixedPoint $native;

    /** @var array<string, array<string, mixed>> FQCN => row */
    public array $rows = [];

    /** @var list<string> */
    public array $violations = [];

    /** @var array<string, bool> */
    private array $throwableCache = [];

    /** @var array<string, bool> */
    private array $valueObjectCandidates = [];

    /** @var array<string, bool> */
    private array $statelessPureCandidates = [];

    public function __construct(private readonly string $composerSrc, private readonly Lists $lists)
    {
    }

    public function run(): void
    {
        $this->sources = new SourceParser();
        $this->sources->parseTree($this->composerSrc);
        $this->sources->finalize();

        $this->closure = new ReachabilityClosure($this->sources, $this->lists);
        $this->closure->run();

        $this->purity = new PurityAnalyzer($this->sources);
        $this->purity->run();

        $this->computeLocalCandidates();

        // The unreachable fixed point needs to know which reachable classes
        // are proxied (constructing one demotes), and candidate demotion
        // needs to know which unreachable classes ended up unsupported.
        // Iterate the two until stable; candidate sets only shrink, so this
        // terminates.
        do {
            $reachableCategories = [];
            foreach ($this->closure->reachable as $fqcn => $bits) {
                $info = $this->sources->classes[$fqcn] ?? null;
                if ($info !== null) {
                    $reachableCategories[$fqcn] = $this->categoryForReachable($fqcn, $info);
                }
            }

            $this->native = new NativeFixedPoint($this->sources, $this->closure, $this->lists, $reachableCategories);
            $this->native->run();
        } while ($this->demoteCandidates());

        foreach ($this->sources->classes as $fqcn => $info) {
            $this->rows[$fqcn] = $this->classify($fqcn, $info);
        }

        $this->applyOverrides();
        $this->collectViolations();
        ksort($this->rows);
    }

    /** @return array<string, mixed> */
    private function classify(string $fqcn, ClassInfo $info): array
    {
        $reachableBits = $this->closure->reachable[$fqcn] ?? 0;
        $row = [
            'fqcn' => $fqcn,
            'kind' => $info->kind,
            'reachable' => $reachableBits !== 0,
            'direction' => $this->directionLabel($reachableBits),
            'category' => null,
            'reasons' => [],
            'attributes' => [],
        ];

        if ($info->writesOwnStaticProps) {
            $row['attributes']['mutable-static'] =
                $this->lists->staticDispositions[$fqcn] ?? 'needs-review';
        }
        if ($this->isThrowable($fqcn)) {
            $row['attributes']['throwable'] = true;
        }

        if ($this->lists->isTwoWorld($fqcn)) {
            $row['category'] = 'two-world';
            $row['reasons'][] = 'two-world.list';

            return $row;
        }

        if ($reachableBits === 0) {
            $row['category'] = $this->native->categories[$fqcn] ?? 'unsupported';
            $row['reasons'] = array_merge(
                $row['reasons'],
                $this->native->reasons[$fqcn] ?? ['not plugin-reachable; every reference resolves to executable code in the child'],
            );

            return $row;
        }

        $category = $this->categoryForReachable($fqcn, $info);
        $row['category'] = $category;
        $row['reasons'][] = match ($category) {
            'php-native' => $this->isThrowable($fqcn)
                ? 'exception class: real definition in the child process, crossed by value'
                : (($this->statelessPureCandidates[$fqcn] ?? false)
                    ? 'stateless pure class: no instance state, all methods pure, real code answers identically in both worlds'
                    : 'constants-only class: pure definitions, no state to share'),
            'contract' => 'interface/abstract type: declaration stub, artifacts depend on direction',
            'rust-snapshot' => 'immutable value object: no state referencing living services, all methods pure',
            'rust-proxy' => 'reachable concrete class holding or reaching shared state',
        };
        if ($category === 'rust-proxy') {
            $row['attributes']['plugin-constructible'] = $this->hasPublicConstructor($info);
        }
        if (in_array($category, ['rust-proxy', 'rust-snapshot'], true)) {
            // Stub static properties cannot be intercepted in PHP (there is
            // no __getStatic); public ones need an explicit decision.
            $statics = [];
            foreach ($info->properties as $prop) {
                if ($prop->static && $prop->visibility === 'public') {
                    $statics[] = $prop->name;
                }
            }
            if ($statics !== []) {
                sort($statics);
                $row['attributes']['public-static-properties'] = $statics;
            }
        }
        if (in_array($category, ['rust-proxy', 'rust-snapshot', 'contract'], true)) {
            $row['methods'] = $this->methodRows($fqcn, $info);
        }

        return $row;
    }

    private function categoryForReachable(string $fqcn, ClassInfo $info): string
    {
        if ($this->isThrowable($fqcn)) {
            return 'php-native';
        }
        // Contract wins over constants-only: a marker interface such as
        // Capability is still implemented by plugins and needs direction
        // attributes, not just its constant-free declaration.
        if ($info->isContractLike() || $info->kind === 'trait') {
            return 'contract';
        }
        if ($this->isConstantsOnly($info)) {
            return 'php-native';
        }
        if ($this->statelessPureCandidates[$fqcn] ?? false) {
            return 'php-native';
        }
        if ($this->valueObjectCandidates[$fqcn] ?? false) {
            return 'rust-snapshot';
        }

        return 'rust-proxy';
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
        //     none of which references a living service -> rust-snapshot
        //   - stateless pure classes carry no instance state at all;
        //     a snapshot has nothing to copy, and the real code answers
        //     identically in both worlds -> php-native
        do {
            $changed = false;
            foreach ($this->closure->reachable as $fqcn => $bits) {
                if (($this->valueObjectCandidates[$fqcn] ?? false) || ($this->statelessPureCandidates[$fqcn] ?? false)) {
                    continue;
                }
                $info = $this->sources->classes[$fqcn] ?? null;
                if ($info === null || $info->isContractLike() || $info->kind !== 'class') {
                    continue;
                }
                if ($this->lists->isTwoWorld($fqcn) || $this->isThrowable($fqcn) || $this->isConstantsOnly($info)) {
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
     * Snapshot classes ship their real method bodies; stateless pure
     * classes run as real code. Either way, what the bodies construct must
     * be locally constructible: another candidate, an exception, real
     * vendor/two-world/builtin code — not a proxied service.
     */
    private function bodyRefsAreLocal(ClassInfo $info): bool
    {
        foreach (array_unique($info->newRefs) as $ref) {
            if ($ref === $info->fqcn || $this->isCandidate($ref) || $this->isThrowable($ref)) {
                continue;
            }
            if (isset($this->closure->reachable[$ref])) {
                $target = $this->sources->classes[$ref] ?? null;
                if ($target !== null && ($this->isConstantsOnly($target) || $this->lists->isTwoWorld($ref))) {
                    continue;
                }

                // Constructs what will be a proxied service.
                return false;
            }
            // Unreachable, vendor, or builtin targets are checked again
            // once the unreachable fixed point has run (demoteCandidates).
        }

        return true;
    }

    private function isCandidate(string $fqcn): bool
    {
        return ($this->valueObjectCandidates[$fqcn] ?? false) || ($this->statelessPureCandidates[$fqcn] ?? false);
    }

    /**
     * Re-check candidates against the unreachable fixed point's outcome:
     * a candidate whose bodies construct or statically reference an
     * unsupported class cannot run locally after all. Returns true when
     * anything was demoted (the caller then reruns the fixed point).
     */
    private function demoteCandidates(): bool
    {
        $demoted = false;
        do {
            $changed = false;
            foreach (array_merge(array_keys($this->valueObjectCandidates), array_keys($this->statelessPureCandidates)) as $fqcn) {
                if (!$this->isCandidate($fqcn)) {
                    continue;
                }
                $info = $this->sources->classes[$fqcn];
                if ($this->bodyRefsAreLocal($info) && !$this->refsUnsupported($info)) {
                    continue;
                }
                unset($this->valueObjectCandidates[$fqcn], $this->statelessPureCandidates[$fqcn]);
                $changed = true;
                $demoted = true;
            }
        } while ($changed);

        return $demoted;
    }

    private function refsUnsupported(ClassInfo $info): bool
    {
        foreach (array_unique(array_merge($info->newRefs, $info->staticRefs)) as $ref) {
            if (($this->native->categories[$ref] ?? null) === 'unsupported') {
                return true;
            }
        }

        return false;
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
                // Vendor or builtin: value-safe only if php-native.
                $vendor = VendorPackages::lookup($type);
                if ($vendor !== null && $vendor['category'] !== 'php-native') {
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

    private function applyOverrides(): void
    {
        foreach ($this->lists->overrides as $fqcn => $override) {
            if (!isset($this->rows[$fqcn])) {
                $this->violations[] = "overrides.list: unknown class $fqcn";
                continue;
            }
            $this->rows[$fqcn]['computedCategory'] = $this->rows[$fqcn]['category'];
            $this->rows[$fqcn]['category'] = $override['category'];
            $this->rows[$fqcn]['reasons'][] = 'override: ' . $override['reason'];
        }
    }

    private function collectViolations(): void
    {
        foreach ($this->rows as $fqcn => $row) {
            if (($row['attributes']['mutable-static'] ?? null) === 'needs-review') {
                $this->violations[] = "$fqcn writes static properties but has no disposition in static-state.list";
            }
        }
        foreach ($this->closure->unknownReachable as $fqcn => $bits) {
            $this->violations[] = "reachable type $fqcn is neither a composer class, a known vendor package, nor a PHP builtin";
        }
    }
}
