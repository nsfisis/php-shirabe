<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class Report
{
    private const BACKLOG_ITEMS = [
        1 => 'guarded classes the Rust side owns: a stub or a codec is missing',
        2 => 'references the upstream code in the worker cannot follow',
        3 => 'worker classes the guard propagation guards',
        4 => 'forwarded members the Rust side does not answer',
        5 => 'real classes whose Rust-side instances cannot cross',
        6 => 'shared static properties no mechanism keeps in sync',
    ];

    public function __construct(private readonly Classifier $classifier)
    {
    }

    public function render(): string
    {
        $vendor = [];
        foreach ($this->classifier->closure->vendorReachable as $fqcn => $bits) {
            $info = VendorPackages::lookup($fqcn);
            $vendor[] = [
                'fqcn' => $fqcn,
                'package' => $info['package'],
                'value' => $this->classifier->foreignValue($fqcn) ?? $info['value'],
                'direction' => $bits === 3 ? 'both' : ($bits === 1 ? 'provided' : 'consumed'),
            ];
        }
        usort($vendor, static fn (array $a, array $b) => strcmp($a['fqcn'], $b['fqcn']));

        $builtins = array_keys($this->classifier->closure->builtinReachable);
        sort($builtins);

        $vendorPackages = [];
        foreach (VendorPackages::PREFIXES as $info) {
            $vendorPackages[$info['package']] = $info['value'];
        }
        ksort($vendorPackages);

        $data = [
            'categories' => $this->counts(),
            'classes' => array_values($this->classifier->rows),
            'vendorPackages' => $vendorPackages,
            'vendorTypes' => $vendor,
            'reachableBuiltins' => $builtins,
            'backlog' => $this->classifier->backlog,
            'violations' => $this->classifier->violations,
        ];

        return json_encode($data, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n";
    }

    public function printSummary(): void
    {
        $counts = $this->counts();
        echo "# Plugin boundary classification\n\n";
        echo "| value | classes |\n|---|---|\n";
        foreach ($counts['value'] as $value => $count) {
            echo "| $value | $count |\n";
        }
        echo "\n| treatment | classes |\n|---|---|\n";
        foreach ($counts['treatment'] as $treatment => $count) {
            echo "| $treatment | $count |\n";
        }
        echo "\n";

        $overridden = array_filter($this->classifier->rows, static fn (array $r) => isset($r['ruleValue']));
        if ($overridden !== []) {
            echo "## Overridden\n\n";
            foreach ($overridden as $row) {
                echo "- {$row['fqcn']}: {$row['ruleValue']} -> {$row['value']}\n";
            }
            echo "\n";
        }

        if ($this->classifier->violations !== []) {
            echo "## Violations\n\n";
            foreach ($this->classifier->violations as $violation) {
                echo "- $violation\n";
            }
            echo "\n";
        }
    }

    /** Prints the backlog as warnings: each entry is a gap between the current treatment and full compatibility. */
    public function printBacklog(): void
    {
        $byItem = [];
        foreach ($this->classifier->backlog as $entry) {
            $byItem[$entry['item']][] = $entry;
        }
        foreach (self::BACKLOG_ITEMS as $item => $title) {
            $entries = $byItem[$item] ?? [];
            if ($entries !== []) {
                fwrite(STDERR, "warning: backlog $item ($title): " . count($entries) . " entries\n");
            }
        }
    }

    /** @return array{value: array<string, int>, treatment: array<string, int>} */
    private function counts(): array
    {
        $values = array_fill_keys(Classifier::VALUES, 0);
        $treatments = array_fill_keys(Classifier::TREATMENTS, 0);
        foreach ($this->classifier->rows as $row) {
            if ($row['value'] !== null) {
                $values[$row['value']]++;
            }
            $treatments[$row['treatment']]++;
        }

        return ['value' => $values, 'treatment' => $treatments];
    }
}
