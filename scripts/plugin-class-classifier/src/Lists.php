<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * The three versioned exception lists. Formats are line-based; `#` starts a
 * comment, blank lines are skipped.
 *
 *   two-world.list    one namespace prefix (or exact FQCN) per line
 *   static-state.list "FQCN memo-cache|seed-once|needs-sync" per line
 *   overrides.list    "FQCN category reason..." per line
 */
final class Lists
{
    /** @var list<string> */
    public array $twoWorldPrefixes = [];

    /** @var array<string, string> FQCN => 'memo-cache' | 'seed-once' | 'needs-sync' */
    public array $staticDispositions = [];

    /** @var array<string, array{category: string, reason: string}> */
    public array $overrides = [];

    public static function load(string $dir): self
    {
        $lists = new self();

        foreach (self::lines("$dir/two-world.list") as $line) {
            $lists->twoWorldPrefixes[] = $line;
        }

        foreach (self::lines("$dir/static-state.list") as $line) {
            $parts = preg_split('/\s+/', $line, 2);
            if (count($parts) !== 2 || !in_array($parts[1], ['memo-cache', 'seed-once', 'needs-sync'], true)) {
                throw new \RuntimeException("static-state.list: malformed line: $line");
            }
            $lists->staticDispositions[$parts[0]] = $parts[1];
        }

        foreach (self::lines("$dir/overrides.list") as $line) {
            $parts = preg_split('/\s+/', $line, 3);
            if (count($parts) !== 3) {
                throw new \RuntimeException("overrides.list: malformed line (need FQCN, category, reason): $line");
            }
            if (!in_array($parts[1], Classifier::CATEGORIES, true)) {
                throw new \RuntimeException("overrides.list: unknown category {$parts[1]}: $line");
            }
            $lists->overrides[$parts[0]] = ['category' => $parts[1], 'reason' => $parts[2]];
        }

        return $lists;
    }

    public function isTwoWorld(string $fqcn): bool
    {
        foreach ($this->twoWorldPrefixes as $prefix) {
            if ($fqcn === $prefix || str_starts_with($fqcn, rtrim($prefix, '\\') . '\\')) {
                return true;
            }
        }

        return false;
    }

    /** @return list<string> */
    private static function lines(string $path): array
    {
        if (!is_file($path)) {
            throw new \RuntimeException("missing exception list: $path");
        }
        $out = [];
        foreach (file($path, FILE_IGNORE_NEW_LINES) as $line) {
            $line = trim($line);
            if ($line === '' || str_starts_with($line, '#')) {
                continue;
            }
            $out[] = $line;
        }

        return $out;
    }
}
