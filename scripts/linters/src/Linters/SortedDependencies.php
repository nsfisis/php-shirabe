<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class SortedDependencies implements Linter
{
    public function name(): string
    {
        return 'sorted_dependencies';
    }

    public function failureIntro(): string
    {
        return "Found unsorted `[dependencies]` / `[dev-dependencies]` in Cargo.toml.\n"
            . 'Entries must be alphabetical, with `shirabe-*` crates listed before others:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::cargoTomls($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            $sections = $this->parseDepSections(file_get_contents($path));

            foreach (['dependencies', 'dev-dependencies'] as $section) {
                $deps = $sections[$section] ?? [];
                if ($deps === []) {
                    continue;
                }

                $expected = $this->sortDepNames($deps);
                if ($deps === $expected) {
                    continue;
                }

                $errors[] = "{$relative} [{$section}]\n"
                    . '    actual:   ' . implode(', ', $deps) . "\n"
                    . '    expected: ' . implode(', ', $expected);
            }
        }

        return $errors;
    }

    /** @return array<string, list<string>> */
    private function parseDepSections(string $content): array
    {
        $sections = [];
        $current = null;

        foreach (explode("\n", $content) as $line) {
            $stripped = rtrim($line, "\r\n");

            if (preg_match('/\A\s*\[([^\]]+)\]\s*\z/', $stripped, $m)) {
                $current = $m[1];
                $sections[$current] ??= [];
                continue;
            }

            if ($current !== null && preg_match('/\A([A-Za-z0-9_-]+)\s*[.=]/', $stripped, $m)) {
                $sections[$current][] = $m[1];
            }
        }

        return $sections;
    }

    /**
     * @param list<string> $deps
     * @return list<string>
     */
    private function sortDepNames(array $deps): array
    {
        $shirabe = [];
        $other = [];

        foreach ($deps as $dep) {
            if (str_starts_with($dep, 'shirabe-')) {
                $shirabe[] = $dep;
            } else {
                $other[] = $dep;
            }
        }
        sort($shirabe);
        sort($other);

        return array_merge($shirabe, $other);
    }
}
