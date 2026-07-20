<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoUseAsAlias implements Linter
{
    private const USE_ALIAS_START_RE = '/\A(?:pub(?:\([^)]*\))?\s+)?use\b/';
    private const PASCAL_CASE_RE = '/\A[A-Z][A-Za-z0-9]*\z/';

    public function name(): string
    {
        return 'no_use_as_alias';
    }

    public function failureIntro(): string
    {
        return "Found `use ... as name` aliases.\n"
            . "Aliasing imports merely to shorten a namespace is forbidden.\n"
            . "Only `as _` (e.g. `use std::io::Write as _;`) and PascalCase renames\n"
            . '(for collision avoidance, e.g. `use foo::Error as FooError;`) are allowed:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findUseAliases($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findUseAliases(string $path, string $relative): array
    {
        $errors = [];
        $inUse = false;
        $braceDepth = 0;

        foreach (file($path) as $idx => $raw) {
            $code = explode('//', $raw, 2)[0];
            $stripped = trim($code);

            if (!$inUse) {
                if (!preg_match(self::USE_ALIAS_START_RE, $stripped)) {
                    continue;
                }
                $inUse = true;
                $braceDepth = 0;
            }

            if (preg_match_all('/\bas\s+([A-Za-z_][A-Za-z0-9_]*)/', $code, $m)) {
                foreach ($m[1] as $name) {
                    if ($name === '_') {
                        continue;
                    }
                    if (preg_match(self::PASCAL_CASE_RE, $name)) {
                        continue;
                    }
                    $errors[] = "{$relative}:" . ($idx + 1) . ": `as {$name}` aliasing in `use` statement";
                }
            }

            $braceDepth += substr_count($code, '{') - substr_count($code, '}');
            if ($braceDepth <= 0 && str_ends_with(rtrim($code), ';')) {
                $inUse = false;
                $braceDepth = 0;
            }
        }

        return $errors;
    }
}
