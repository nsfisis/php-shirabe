<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\QualifiedPathScanner;

/**
 * `super` paths are allowed only as `use super::*;` in test code: anywhere in an integration test
 * file (`crates/*\/tests/`), or in a `#[cfg(test)]` module under `src/`. Everything else names the
 * item with a `crate::` path.
 */
final class NoSuperPath implements Linter
{
    private const USE_START_RE = '/\A(?:pub(?:\([^)]*\))?\s+)?use\b/';
    private const MOD_START_RE = '/\A\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?:r#)?[A-Za-z_][A-Za-z0-9_]*\s*\{/';

    public function name(): string
    {
        return 'no_super_path';
    }

    public function failureIntro(): string
    {
        return "Found `super` paths.\n"
            . 'The only allowed form is `use super::*;` in test code (a `#[cfg(test)]` module or an '
            . 'integration test file). Name the item with a `crate::` path instead:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            $src = file_get_contents($path);
            foreach (QualifiedPathScanner::find($src, 'super') as $found) {
                $errors[] = "{$relative}:{$found['line']}: `{$found['path']}`";
            }

            $isTestFile = (bool) preg_match('#\Acrates/[^/]+/tests/#', $relative);
            array_push($errors, ...$this->findSuperUses(file($path), $relative, $isTestFile));
        }

        return $errors;
    }

    /**
     * @param list<string> $lines
     * @return list<string>
     */
    private function findSuperUses(array $lines, string $relative, bool $isTestFile): array
    {
        $errors = [];
        $buffer = null;
        $startIdx = null;

        foreach ($lines as $idx => $raw) {
            $stripped = trim(explode('//', $raw, 2)[0]);

            if ($buffer === null) {
                if (!preg_match(self::USE_START_RE, $stripped)) {
                    continue;
                }
                $buffer = '';
                $startIdx = $idx;
            }

            $buffer .= ' ' . $stripped;
            if (!str_contains($buffer, ';')) {
                continue;
            }

            $statement = trim(preg_replace('/\s+/', ' ', $buffer));
            $buffer = null;

            if (!preg_match('/\bsuper\b/', $statement)) {
                continue;
            }

            $line = $startIdx + 1;
            if ($statement !== 'use super::*;') {
                $errors[] = "{$relative}:{$line}: `{$statement}` (only `use super::*;` is allowed)";
            } elseif (!$isTestFile && !$this->isInCfgTestModule($lines, $startIdx)) {
                $errors[] = "{$relative}:{$line}: `use super::*;` outside a `#[cfg(test)]` module";
            }
        }

        return $errors;
    }

    /** @param list<string> $lines */
    private function isInCfgTestModule(array $lines, int $useIdx): bool
    {
        for ($i = $useIdx - 1; $i >= 0; $i--) {
            if (!preg_match(self::MOD_START_RE, $lines[$i])) {
                continue;
            }

            for ($j = $i - 1; $j >= 0; $j--) {
                $attr = trim(explode('//', $lines[$j], 2)[0]);
                if ($attr === '#[cfg(test)]') {
                    return true;
                }
                if (preg_match('/[;{}]\z/', $attr)) {
                    return false;
                }
            }

            return false;
        }

        return false;
    }
}
