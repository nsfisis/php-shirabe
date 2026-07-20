<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Computes the plugin-reachable type set with direction marks, per the
 * "Mechanical rules" steps 2-4 in docs/dev/plugin-class-classification.md.
 */
final class ReachabilityClosure
{
    public const PROVIDED = 1;
    public const CONSUMED = 2;

    /** @var array<string, int> composer-src FQCN => direction bits */
    public array $reachable = [];

    /** @var array<string, int> vendor FQCN => direction bits */
    public array $vendorReachable = [];

    /** @var array<string, int> global-namespace (builtin) name => direction bits */
    public array $builtinReachable = [];

    /** @var array<string, int> unknown FQCN => direction bits (reported) */
    public array $unknownReachable = [];

    /** @var array<string, list<string>> FQCN => transitive subtypes */
    private array $descendants = [];

    /** @var list<array{string, int}> */
    private array $worklist = [];

    public function __construct(
        private readonly SourceParser $sources,
        private readonly Lists $lists,
    ) {
        $this->buildDescendants();
    }

    public function run(): void
    {
        foreach ($this->sources->classes as $fqcn => $info) {
            if (str_starts_with($fqcn, 'Composer\\Plugin\\')) {
                $this->add($fqcn, $info->kind === 'interface' ? self::CONSUMED : self::PROVIDED);
            }
        }
        $this->add('Composer\\EventDispatcher\\EventSubscriberInterface', self::CONSUMED);
        $this->add('Composer\\EventDispatcher\\Event', self::PROVIDED);
        foreach ($this->descendants['Composer\\EventDispatcher\\Event'] ?? [] as $sub) {
            $this->add($sub, self::PROVIDED);
        }

        while ($this->worklist !== []) {
            [$fqcn, $bits] = array_pop($this->worklist);
            $this->expand($fqcn, $bits);
        }
    }

    private function add(string $fqcn, int $bits): void
    {
        if ($fqcn === '') {
            return;
        }

        if (isset($this->sources->classes[$fqcn])) {
            $current = $this->reachable[$fqcn] ?? 0;
            $new = $current | $bits;
            if ($new !== $current) {
                $this->reachable[$fqcn] = $new;
                $this->worklist[] = [$fqcn, $new & ~$current];
            }

            return;
        }

        if (VendorPackages::lookup($fqcn) !== null) {
            $this->vendorReachable[$fqcn] = ($this->vendorReachable[$fqcn] ?? 0) | $bits;

            return;
        }

        if (!str_contains($fqcn, '\\')) {
            $this->builtinReachable[$fqcn] = ($this->builtinReachable[$fqcn] ?? 0) | $bits;

            return;
        }

        $this->unknownReachable[$fqcn] = ($this->unknownReachable[$fqcn] ?? 0) | $bits;
    }

    /** Expand newly gained direction bits of a composer-src type. */
    private function expand(string $fqcn, int $newBits): void
    {
        // Two-world classes do not extend the shared boundary: the child
        // process carries their real implementation wholesale, so their
        // members are world-2-local, not graph seams.
        if ($this->lists->isTwoWorld($fqcn)) {
            return;
        }

        $info = $this->sources->classes[$fqcn];

        // Hierarchy carries the same direction both ways: ancestors so that
        // instanceof works on the stub side, descendants because any
        // concrete subtype can be the runtime instance behind the type.
        if ($info->parent !== null) {
            $this->add($info->parent, $newBits);
        }
        foreach ($info->interfaces as $iface) {
            $this->add($iface, $newBits);
        }
        foreach ($this->descendants[$fqcn] ?? [] as $sub) {
            $this->add($sub, $newBits);
        }

        // The inverted expansion (params->provided, returns->consumed)
        // models Composer calling a plugin's implementation, so it only
        // applies to plugin-implementable types. A concrete class that
        // gained a consumed mark (a plugin can construct and pass one in)
        // still executes Composer's own method bodies.
        $expandBits = $info->isContractLike() || $info->kind === 'trait'
            ? $newBits
            : ($newBits !== 0 ? self::PROVIDED : 0);

        foreach ($info->methods as $method) {
            if ($method->visibility === 'private') {
                continue;
            }
            $this->expandMethod($method, $expandBits);
        }

        foreach ($info->properties as $prop) {
            if ($prop->visibility === 'private') {
                continue;
            }
            foreach ($this->propTypes($prop) as $type) {
                // Property values are read by (sub)classing plugin code:
                // they flow toward PHP regardless of the owner's direction.
                $this->add($type, self::PROVIDED);
            }
        }
    }

    private function expandMethod(MethodInfo $method, int $ownerBits): void
    {
        // On a provided type, plugins call the methods: arguments flow
        // PHP->Rust (consumed), results flow Rust->PHP (provided). On a
        // consumed type, Composer calls the plugin's implementation: the
        // directions invert.
        foreach ([self::PROVIDED => [self::CONSUMED, self::PROVIDED], self::CONSUMED => [self::PROVIDED, self::CONSUMED]] as $ownerDir => [$paramDir, $returnDir]) {
            if (($ownerBits & $ownerDir) === 0) {
                continue;
            }

            foreach ($method->params as $param) {
                foreach ($param->classTypes as $type) {
                    $this->add($type, $paramDir);
                }
                if ($param->expandable || $param->classTypes === []) {
                    foreach ($param->docblockTypes as $type) {
                        $this->add($type, $paramDir);
                    }
                }
            }

            foreach ($method->returnClassTypes as $type) {
                $this->add($type, $returnDir);
            }
            if ($method->returnExpandable || $method->returnClassTypes === []) {
                foreach ($method->docblockReturnTypes as $type) {
                    $this->add($type, $returnDir);
                }
            }
            foreach ($method->docblockThrowsTypes as $type) {
                $this->add($type, $returnDir);
            }
        }
    }

    /** @return list<string> */
    private function propTypes(PropertyInfo $prop): array
    {
        $types = $prop->classTypes;
        if ($prop->expandable || $types === []) {
            $types = array_merge($types, $prop->docblockTypes);
        }

        return $types;
    }

    private function buildDescendants(): void
    {
        $direct = [];
        foreach ($this->sources->classes as $fqcn => $info) {
            if ($info->parent !== null) {
                $direct[$info->parent][] = $fqcn;
            }
            foreach ($info->interfaces as $iface) {
                $direct[$iface][] = $fqcn;
            }
        }

        foreach (array_keys($direct) as $root) {
            $seen = [];
            $stack = $direct[$root];
            while ($stack !== []) {
                $cur = array_pop($stack);
                if (isset($seen[$cur])) {
                    continue;
                }
                $seen[$cur] = true;
                foreach ($direct[$cur] ?? [] as $child) {
                    $stack[] = $child;
                }
            }
            $this->descendants[$root] = array_keys($seen);
            sort($this->descendants[$root]);
        }
    }
}
