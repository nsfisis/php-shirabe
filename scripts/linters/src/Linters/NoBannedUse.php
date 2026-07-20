<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoBannedUse implements Linter
{
    private const BANNED_USE_PATHS = [
        'anyhow::Result',
        'std::any::Any',
        'std::cell::RefCell',
        'std::io::Read',
        'std::io::Write',
        'std::process::Command',
        'std::rc::Rc',
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

        foreach (file($path) as $idx => $raw) {
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
                if (!in_array($full, self::BANNED_USE_PATHS, true)) {
                    continue;
                }
                $errors[] = "{$relative}:" . ($startIdx + 1) . ": `use {$full}` is banned (fully qualify as `{$full}` instead)";
            }

            $buffer = null;
        }

        return array_values(array_unique($errors));
    }

    /** @return list<string> */
    private function expandUseTree(?string $tree): array
    {
        if ($tree === null) {
            return [];
        }

        $tree = trim($tree);
        $brace = strpos($tree, '{');

        if ($brace === false) {
            $stripped = $this->stripUseAlias($tree);

            return $stripped === null || $stripped === '' ? [] : [$stripped];
        }

        $prefix = trim(preg_replace('/::\s*\z/', '', substr($tree, 0, $brace)));
        $inner = preg_replace('/\}\s*\z/', '', substr($tree, $brace + 1));

        $result = [];
        foreach ($this->splitTopLevel($inner) as $child) {
            foreach ($this->expandUseTree($child) as $sub) {
                if ($sub === '' || $sub === 'self') {
                    $result[] = $prefix;
                } elseif ($prefix === '') {
                    $result[] = $sub;
                } else {
                    $result[] = "{$prefix}::{$sub}";
                }
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

    /** @return list<string> */
    private function splitTopLevel(string $str): array
    {
        $parts = [];
        $current = '';
        $depth = 0;

        foreach (str_split($str) as $ch) {
            switch ($ch) {
                case '{':
                    $depth++;
                    $current .= $ch;
                    break;
                case '}':
                    $depth--;
                    $current .= $ch;
                    break;
                case ',':
                    if ($depth === 0) {
                        $parts[] = $current;
                        $current = '';
                    } else {
                        $current .= $ch;
                    }
                    break;
                default:
                    $current .= $ch;
            }
        }
        $parts[] = $current;

        return array_values(array_filter(array_map('trim', $parts), static fn (string $p): bool => $p !== ''));
    }
}
