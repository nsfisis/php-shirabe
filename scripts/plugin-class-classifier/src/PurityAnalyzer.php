<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Per-method pure/mutator decision (see "pure/mutator" in
 * docs/dev/plugin-class-classification.md): a method is a mutator iff it
 * writes $this directly, passes a $this-rooted expression into a by-ref
 * parameter, contains a statically unresolvable call that a $this-rooted
 * expression escapes into, or (transitively) calls a mutator method on the
 * same object.
 */
final class PurityAnalyzer
{
    /** @var array<string, string> "Fqcn::lowername" => 'pure' | 'mutator' | 'n/a' */
    public array $verdicts = [];

    public function __construct(private readonly SourceParser $sources)
    {
    }

    public function run(): void
    {
        // Seed with local evidence.
        foreach ($this->sources->classes as $fqcn => $info) {
            foreach ($info->methods as $lname => $method) {
                $key = "$fqcn::$lname";
                if ($method->static || $method->abstract) {
                    $this->verdicts[$key] = 'n/a';
                    continue;
                }
                $this->verdicts[$key] = $this->locallyMutates($fqcn, $method) ? 'mutator' : 'pure';
            }
        }

        // Propagate through same-object calls until stable.
        do {
            $changed = false;
            foreach ($this->sources->classes as $fqcn => $info) {
                foreach ($info->methods as $lname => $method) {
                    $key = "$fqcn::$lname";
                    if ($this->verdicts[$key] !== 'pure') {
                        continue;
                    }
                    foreach ($method->selfCalls as $call) {
                        $callee = $this->resolveMethod($fqcn, $call['name']);
                        if ($callee === null) {
                            // Magic __call or a method we cannot see.
                            $this->verdicts[$key] = 'mutator';
                            $changed = true;
                            break;
                        }
                        [$calleeClass, $calleeMethod] = $callee;
                        $calleeVerdict = $this->verdicts["$calleeClass::" . strtolower($calleeMethod->name)] ?? 'mutator';
                        if ($calleeVerdict === 'mutator') {
                            $this->verdicts[$key] = 'mutator';
                            $changed = true;
                            break;
                        }
                        if (array_intersect($call['thisArgs'], $calleeMethod->byRefParamPositions()) !== []) {
                            $this->verdicts[$key] = 'mutator';
                            $changed = true;
                            break;
                        }
                    }
                }
            }
        } while ($changed);
    }

    private function locallyMutates(string $fqcn, MethodInfo $method): bool
    {
        if ($method->mutatesThisDirectly || $method->thisEscapesUnresolved) {
            return true;
        }

        foreach ($method->externalCalls as $call) {
            $positions = $this->byRefPositionsOf($call['class'], $call['method']);
            if ($positions === null) {
                // Callee signature unknown: conservative.
                return true;
            }
            if (array_intersect($call['thisArgs'], $positions) !== []) {
                return true;
            }
        }

        return false;
    }

    /** @return list<int>|null null when the signature cannot be resolved */
    private function byRefPositionsOf(string $class, string $lowerMethod): ?array
    {
        $resolved = $this->resolveMethod($class, $lowerMethod);
        if ($resolved !== null) {
            return $resolved[1]->byRefParamPositions();
        }

        $vendor = VendorPackages::BY_REF[$class] ?? null;
        if ($vendor !== null) {
            return $vendor[$lowerMethod] ?? [];
        }
        if (VendorPackages::lookup($class) !== null) {
            // Vendor packages other than the ones tabled expose no by-ref
            // parameters in APIs composer calls.
            return [];
        }

        return null;
    }

    /** @return array{string, MethodInfo}|null resolved (declaring class, method) */
    private function resolveMethod(string $class, string $lowerMethod): ?array
    {
        $seen = [];
        $current = $class;
        while ($current !== null && !isset($seen[$current])) {
            $seen[$current] = true;
            $info = $this->sources->classes[$current] ?? null;
            if ($info === null) {
                return null;
            }
            if (isset($info->methods[$lowerMethod])) {
                return [$current, $info->methods[$lowerMethod]];
            }
            $current = $info->parent;
        }

        return null;
    }
}
