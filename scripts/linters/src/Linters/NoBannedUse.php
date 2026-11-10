<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\UseTree;

final class NoBannedUse implements Linter
{
    /**
     * Regex patterns (without delimiters) matched against the whole expanded `use` path,
     * e.g. `foo::.*` bans every item under `foo`.
     */
    private const BANNED_USE_PATHS = [
        'anyhow::.*',
        'bstr::ByteSlice',
        'bstr::ByteVec',
        'chrono::.*',
        'criterion::.*',
        'futures::.*',
        'proc_macro2::.*',
        'quote::.*',
        'serde::.*',
        'serial_test::.*',
        'std::.*',
        'syn::.*',
        'tempfile::.*',
        'zip::.*',
    ];

    private const USE_START_RE = '/\A(?:pub(?:\([^)]*\))?\s+)?use\b/';

    public function name(): string
    {
        return 'no_banned_use';
    }

    public function failureIntro(): string
    {
        return "Found banned `use` imports.\n"
            . "These items must always be referenced by their fully-qualified path.\n"
            . 'For imports to use trait methods, use `as _` (e.g., `use std::io::Write as _;`).';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findBannedUses($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findBannedUses(string $path, string $relative): array
    {
        $errors = [];
        $buffer = null;
        $startIdx = null;
        $lines = file($path);
        $localModules = $this->findLocalModules($lines);

        foreach ($lines as $idx => $raw) {
            $code = explode('//', $raw, 2)[0];
            $stripped = trim($code);

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

            preg_match('/\buse\s+(.*?);/s', $buffer, $m);
            $tree = $m[1] ?? null;

            foreach ($this->expandUseTree($tree) as $full) {
                if (!$this->isBanned($full, $localModules)) {
                    continue;
                }
                $errors[] = "{$relative}:" . ($startIdx + 1) . ": `use {$full}` is banned (fully qualify as `{$full}` instead)";
            }

            $buffer = null;
        }

        return array_values(array_unique($errors));
    }

    /**
     * Names of the modules declared in this file. A `use` path starting with one of them refers to
     * that module, not to an external crate of the same name.
     *
     * @param list<string> $lines
     * @return list<string>
     */
    private function findLocalModules(array $lines): array
    {
        $modules = [];
        foreach ($lines as $raw) {
            $code = trim(explode('//', $raw, 2)[0]);
            if (preg_match('/\A(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\b/', $code, $m)) {
                $modules[] = $m[1];
            }
        }

        return $modules;
    }

    /** @param list<string> $localModules */
    private function isBanned(string $path, array $localModules): bool
    {
        if (in_array(explode('::', $path, 2)[0], $localModules, true)) {
            return false;
        }

        foreach (self::BANNED_USE_PATHS as $pattern) {
            if (preg_match('~\\A(?:' . $pattern . ')\\z~', $path)) {
                return true;
            }
        }

        return false;
    }

    /** @return list<string> */
    private function expandUseTree(?string $tree): array
    {
        if ($tree === null) {
            return [];
        }

        $result = [];
        foreach (UseTree::expand($tree) as $path) {
            $stripped = $this->stripUseAlias($path);
            if ($stripped !== null && $stripped !== '') {
                $result[] = $stripped;
            }
        }

        return $result;
    }

    private function stripUseAlias(string $segment): ?string
    {
        if (preg_match('/\s+as\s+_\s*\z/', $segment)) {
            return null;
        }

        return trim(preg_replace('/\s+as\s+\S+\s*\z/', '', $segment));
    }
}
