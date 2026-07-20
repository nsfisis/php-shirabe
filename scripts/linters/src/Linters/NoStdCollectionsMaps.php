<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoStdCollectionsMaps implements Linter
{
    private const BANNED_MAP_NAMES = ['HashMap', 'HashSet', 'BTreeMap', 'BTreeSet'];

    public function name(): string
    {
        return 'no_std_collections_maps';
    }

    public function failureIntro(): string
    {
        return "Found uses of `std::collections::{HashMap, HashSet, BTreeMap, BTreeSet}`.\n"
            . 'Use `indexmap::IndexMap` / `indexmap::IndexSet` instead:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findStdMapUsages($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findStdMapUsages(string $path, string $relative): array
    {
        $errors = [];

        foreach (file($path) as $idx => $raw) {
            $code = explode('//', $raw, 2)[0];

            if (preg_match_all('/\bstd::collections::(HashMap|HashSet|BTreeMap|BTreeSet)\b/', $code, $m)) {
                foreach ($m[1] as $name) {
                    $errors[] = "{$relative}:" . ($idx + 1) . ": use of `std::collections::{$name}` (use `indexmap::" . self::indexmapReplacement($name) . '` instead)';
                }
            }

            if (preg_match_all('/\bstd::collections::\{([^}]*)\}/', $code, $m)) {
                foreach ($m[1] as $group) {
                    foreach (explode(',', $group) as $entry) {
                        $name = preg_split('/\s+as\s+/', trim($entry))[0];
                        if (!in_array($name, self::BANNED_MAP_NAMES, true)) {
                            continue;
                        }
                        $errors[] = "{$relative}:" . ($idx + 1) . ": import of `std::collections::{$name}` (use `indexmap::" . self::indexmapReplacement($name) . '` instead)';
                    }
                }
            }
        }

        return array_values(array_unique($errors));
    }

    private static function indexmapReplacement(string $name): string
    {
        return match ($name) {
            'HashMap', 'BTreeMap' => 'IndexMap',
            'HashSet', 'BTreeSet' => 'IndexSet',
            default => throw new \LogicException("unexpected map name: {$name}"),
        };
    }
}
