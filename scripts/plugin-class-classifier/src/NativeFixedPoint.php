<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Decides php-native vs unsupported for classes the closure never reached,
 * per "Mechanical rules" step 6: leaf-first fixed point over body
 * references.
 *
 * What a reference may legally target from real PHP running in the child
 * process:
 *
 *   - `new X` works when X exists there as executable-or-local code: a
 *     php-native class, a two-world class (real console code), a vendor
 *     class (real package code), a rust-snapshot value object (values are
 *     built locally), or a PHP builtin. Constructing a rust-proxy service
 *     is the unresolved dual-instantiation case and demotes.
 *   - `X::method()` / `X::$prop` additionally works when X is any stubbed
 *     reachable class (proxy stubs carry static methods as RPC), so only
 *     unsupported peers and unknown types demote.
 */
final class NativeFixedPoint
{
    /** @var array<string, string> unreachable FQCN => 'php-native' | 'unsupported' */
    public array $categories = [];

    /** @var array<string, list<string>> FQCN => reasons for demotion */
    public array $reasons = [];

    /** @var list<string> classes writing static props with no filed disposition */
    public array $unfiledStaticState = [];

    public function __construct(
        private readonly SourceParser $sources,
        private readonly ReachabilityClosure $closure,
        private readonly Lists $lists,
        /** @var array<string, string> reachable FQCN => preliminary category */
        private readonly array $reachableCategories,
    ) {
    }

    public function run(): void
    {
        $universe = [];
        foreach ($this->sources->classes as $fqcn => $info) {
            if (isset($this->closure->reachable[$fqcn]) || $this->lists->isTwoWorld($fqcn)) {
                continue;
            }
            if (isset($this->lists->overrides[$fqcn])) {
                // The override fixes this class's category; it does not
                // participate in the fixed point.
                continue;
            }
            $universe[$fqcn] = $info;
        }

        foreach ($universe as $fqcn => $info) {
            $this->categories[$fqcn] = 'php-native';
            if ($info->writesOwnStaticProps) {
                $disposition = $this->lists->staticDispositions[$fqcn] ?? null;
                if ($disposition === null) {
                    $this->unfiledStaticState[] = $fqcn;
                    $this->demote($fqcn, 'mutable static state with no filed disposition');
                } elseif ($disposition === 'needs-sync') {
                    $this->demote($fqcn, 'mutable static state shared with the Rust side (needs-sync)');
                }
            }
        }

        do {
            $changed = false;
            foreach ($universe as $fqcn => $info) {
                if ($this->categories[$fqcn] === 'unsupported') {
                    continue;
                }
                $bad = $this->firstBadRef($info);
                if ($bad !== null) {
                    $this->demote($fqcn, $bad);
                    $changed = true;
                }
            }
        } while ($changed);
    }

    private function firstBadRef(ClassInfo $info): ?string
    {
        foreach (array_unique($info->newRefs) as $ref) {
            $problem = $this->checkTarget($info, $ref, true);
            if ($problem !== null) {
                return $problem;
            }
        }
        foreach (array_unique($info->staticRefs) as $ref) {
            $problem = $this->checkTarget($info, $ref, false);
            if ($problem !== null) {
                return $problem;
            }
        }

        return null;
    }

    private function checkTarget(ClassInfo $info, string $ref, bool $isNew): ?string
    {
        if ($ref === $info->fqcn) {
            return null;
        }

        $category = $this->lists->overrides[$ref]['category']
            ?? $this->reachableCategories[$ref]
            ?? null;
        if ($category !== null) {
            if ($isNew && $category === 'rust-proxy') {
                return "constructs proxied service $ref (dual instantiation unresolved)";
            }

            return null;
        }

        if ($this->lists->isTwoWorld($ref)) {
            return null;
        }

        if (isset($this->sources->classes[$ref])) {
            if (($this->categories[$ref] ?? 'unsupported') === 'unsupported') {
                return ($isNew ? 'constructs' : 'statically references') . " unsupported type $ref";
            }

            return null;
        }

        if (VendorPackages::lookup($ref) !== null) {
            // Vendor packages ship as real PHP in the child process
            // regardless of their bridging category.
            return null;
        }

        if (!str_contains($ref, '\\')) {
            // Global namespace: PHP builtin.
            return null;
        }

        return ($isNew ? 'constructs' : 'statically references') . " unknown type $ref";
    }

    private function demote(string $fqcn, string $reason): void
    {
        $this->categories[$fqcn] = 'unsupported';
        $this->reasons[$fqcn][] = $reason;
    }
}
