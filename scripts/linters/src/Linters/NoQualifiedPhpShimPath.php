<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

/**
 * Items of `shirabe_php_shim` stand for PHP built-ins, which PHP code calls unqualified, so they
 * are imported with `use` rather than spelled out at every call site. Macros are exempt: the body
 * of a `macro_rules!` expands in the caller's scope and cannot rely on its imports, and a macro is
 * invoked through its path.
 */
final class NoQualifiedPhpShimPath implements Linter
{
    private const QUALIFIED_RE = '/\Ashirabe_php_shim\s*::\s*([A-Za-z_][A-Za-z0-9_]*)(\s*!)?/';

    public function name(): string
    {
        return 'no_qualified_php_shim_path';
    }

    public function failureIntro(): string
    {
        return "Found `shirabe_php_shim::` paths outside `use` statements.\n"
            . 'Import the item with `use shirabe_php_shim::...;` instead '
            . '(macro invocations and `macro_rules!` bodies are exempt):';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findQualifiedPaths(file_get_contents($path), $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findQualifiedPaths(string $src, string $relative): array
    {
        $errors = [];
        $len = strlen($src);
        $line = 1;
        $i = 0;

        while ($i < $len) {
            $skipped = $this->skipNonCode($src, $i);
            if ($skipped !== $i) {
                $line += substr_count($src, "\n", $i, $skipped - $i);
                $i = $skipped;
                continue;
            }

            $ch = $src[$i];
            if ($ch === "\n") {
                $line++;
                $i++;
                continue;
            }

            if (!ctype_alpha($ch) && $ch !== '_') {
                $i++;
                continue;
            }

            preg_match('/\A[A-Za-z0-9_]+/', substr($src, $i, 64), $m);
            $word = $m[0];

            if ($word === 'use') {
                $end = $this->skipUntilSemicolon($src, $i);
            } elseif ($word === 'macro_rules' && ($src[$i + strlen($word)] ?? '') === '!') {
                $end = $this->skipMacroRules($src, $i + strlen($word) + 1);
            } elseif ($word === 'shirabe_php_shim'
                && preg_match(self::QUALIFIED_RE, substr($src, $i, 256), $q)
                && !isset($q[2])
            ) {
                $errors[] = "{$relative}:{$line}: `shirabe_php_shim::{$q[1]}` (import it with `use`)";
                $end = $i + strlen($word);
            } else {
                $end = $i + strlen($word);
            }

            $line += substr_count($src, "\n", $i, $end - $i);
            $i = $end;
        }

        return $errors;
    }

    /** Returns the offset past a comment, string or char literal starting at $i, or $i itself. */
    private function skipNonCode(string $src, int $i): int
    {
        $rest = substr($src, $i, 256);

        if (str_starts_with($rest, '//')) {
            $end = strpos($src, "\n", $i);

            return $end === false ? strlen($src) : $end;
        }
        if (str_starts_with($rest, '/*')) {
            $depth = 0;
            $len = strlen($src);
            while ($i < $len) {
                if (substr($src, $i, 2) === '/*') {
                    $depth++;
                    $i += 2;
                } elseif (substr($src, $i, 2) === '*/') {
                    $depth--;
                    $i += 2;
                    if ($depth === 0) {
                        return $i;
                    }
                } else {
                    $i++;
                }
            }

            return $len;
        }

        $prev = $i > 0 ? $src[$i - 1] : ' ';
        if (ctype_alnum($prev) || $prev === '_') {
            return $i;
        }

        if (preg_match('/\Ab?r(#*)"/', $rest, $m)) {
            $close = strpos($src, '"' . $m[1], $i + strlen($m[0]));

            return $close === false ? strlen($src) : $close + 1 + strlen($m[1]);
        }
        if (preg_match('/\Ab?"/', $rest, $m)) {
            $j = $i + strlen($m[0]);
            $len = strlen($src);
            while ($j < $len && $src[$j] !== '"') {
                $j += $src[$j] === '\\' ? 2 : 1;
            }

            return $j + 1;
        }
        if (preg_match("/\\Ab?'(?:\\\\(?:u\\{[0-9A-Fa-f]+\\}|x[0-9A-Fa-f]{2}|.)|[^\\\\'])'/u", $rest, $m)) {
            return $i + strlen($m[0]);
        }

        return $i;
    }

    private function skipUntilSemicolon(string $src, int $i): int
    {
        $len = strlen($src);
        while ($i < $len && $src[$i] !== ';') {
            $skipped = $this->skipNonCode($src, $i);
            $i = $skipped === $i ? $i + 1 : $skipped;
        }

        return $i;
    }

    /** Skips the macro name and the delimited body following `macro_rules!`. */
    private function skipMacroRules(string $src, int $i): int
    {
        $len = strlen($src);
        while ($i < $len && !in_array($src[$i], ['{', '(', '['], true)) {
            $i++;
        }

        $depth = 0;
        while ($i < $len) {
            $skipped = $this->skipNonCode($src, $i);
            if ($skipped !== $i) {
                $i = $skipped;
                continue;
            }
            if (in_array($src[$i], ['{', '(', '['], true)) {
                $depth++;
            } elseif (in_array($src[$i], ['}', ')', ']'], true)) {
                $depth--;
                if ($depth === 0) {
                    return $i + 1;
                }
            }
            $i++;
        }

        return $len;
    }
}
