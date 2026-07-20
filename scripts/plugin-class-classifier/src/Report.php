<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class Report
{
    public function __construct(private readonly Classifier $classifier)
    {
    }

    public function writeJson(string $path): void
    {
        $vendor = [];
        foreach ($this->classifier->closure->vendorReachable as $fqcn => $bits) {
            $info = VendorPackages::lookup($fqcn);
            $vendor[] = [
                'fqcn' => $fqcn,
                'package' => $info['package'],
                'category' => $info['category'],
                'direction' => $bits === 3 ? 'both' : ($bits === 1 ? 'provided' : 'consumed'),
            ];
        }
        usort($vendor, static fn (array $a, array $b) => strcmp($a['fqcn'], $b['fqcn']));

        $builtins = array_keys($this->classifier->closure->builtinReachable);
        sort($builtins);

        $vendorPackages = [];
        foreach (VendorPackages::PREFIXES as $prefix => $info) {
            $vendorPackages[$info['package']] = $info['category'];
        }
        ksort($vendorPackages);

        $data = [
            'categories' => $this->countByCategory(),
            'classes' => array_values($this->classifier->rows),
            'vendorPackages' => $vendorPackages,
            'vendorTypes' => $vendor,
            'reachableBuiltins' => $builtins,
            'violations' => $this->classifier->violations,
        ];

        $json = json_encode($data, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
        if (file_put_contents($path, $json . "\n") === false) {
            throw new \RuntimeException("cannot write $path");
        }
    }

    public function printSummary(): void
    {
        $counts = $this->countByCategory();
        echo "# Plugin boundary classification\n\n";
        echo "| category | classes |\n|---|---|\n";
        foreach ($counts as $category => $count) {
            echo "| $category | $count |\n";
        }

        echo "\n## Reachable surface\n\n";
        foreach (['rust-proxy', 'rust-snapshot', 'contract'] as $category) {
            $names = [];
            foreach ($this->classifier->rows as $row) {
                if ($row['category'] === $category && $row['reachable']) {
                    $names[] = $row['fqcn'] . ($row['direction'] === 'both' ? ' (both)' : '');
                }
            }
            echo '### ' . $category . ' (' . count($names) . ")\n\n";
            foreach ($names as $name) {
                echo "- $name\n";
            }
            echo "\n";
        }

        $overridden = array_filter($this->classifier->rows, static fn (array $r) => isset($r['computedCategory']));
        if ($overridden !== []) {
            echo "## Overridden\n\n";
            foreach ($overridden as $row) {
                echo "- {$row['fqcn']}: {$row['computedCategory']} -> {$row['category']}\n";
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

    /** @return array<string, int> */
    private function countByCategory(): array
    {
        $counts = array_fill_keys(Classifier::CATEGORIES, 0);
        foreach ($this->classifier->rows as $row) {
            $counts[$row['category']]++;
        }

        return $counts;
    }
}
