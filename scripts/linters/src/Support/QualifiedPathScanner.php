<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

/**
 * Finds paths rooted at a given first segment (e.g. `shirabe_php_shim::foo::Bar`) in Rust source,
 * outside comments, string and char literals, `use` declarations, `macro_rules!` bodies and
 * restricted visibilities (`pub(in ...)`). Paths that name a macro invocation (`root::foo!`) are
 * reported too.
 */
final class QualifiedPathScanner
{
    private const SEGMENT_RE = '/\G\s*::\s*((?:r#)?[A-Za-z_][A-Za-z0-9_]*)/';

    /** Bytes that can start a word, a comment, a literal, or a line. */
    private const STOP_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_/\"'\n";

    /** @return list<array{path: string, line: int, offset: int, length: int}> */
    public static function find(string $src, string $root): array
    {
        $found = [];
        if (!str_contains($src, $root)) {
            return $found;
        }

        $len = strlen($src);
        $line = 1;
        $i = 0;

        while ($i < $len) {
            $skipped = self::skipNonCode($src, $i);
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
                $i += strcspn($src, self::STOP_CHARS, $i);
                continue;
            }

            $wordLen = strspn($src, 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_', $i);
            $word = substr($src, $i, $wordLen);
            $afterWord = $i + $wordLen;
            $prev = $i > 0 ? $src[$i - 1] : ' ';

            if ($word === 'use') {
                $end = self::skipUntilSemicolon($src, $i);
            } elseif ($word === 'macro_rules' && ($src[$afterWord] ?? '') === '!') {
                $end = self::skipDelimited($src, $afterWord + 1);
            } elseif ($word === 'pub' && preg_match('/\G\s*\(/', $src, $m, 0, $afterWord)) {
                $end = self::skipDelimited($src, $afterWord);
            } elseif ($word === $root && $prev !== '$') {
                $end = $afterWord;
                $path = $word;
                while (preg_match(self::SEGMENT_RE, $src, $s, 0, $end)) {
                    $path .= "::{$s[1]}";
                    $end += strlen($s[0]);
                }
                if ($path !== $word) {
                    $found[] = ['path' => $path, 'line' => $line, 'offset' => $i, 'length' => $end - $i];
                }
            } else {
                $end = $afterWord;
            }

            $line += substr_count($src, "\n", $i, $end - $i);
            $i = $end;
        }

        return $found;
    }

    /** Returns the offset past a comment, string or char literal starting at $i, or $i itself. */
    private static function skipNonCode(string $src, int $i): int
    {
        $ch = $src[$i];
        if ($ch !== '/' && $ch !== '"' && $ch !== "'" && $ch !== 'b' && $ch !== 'r') {
            return $i;
        }

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

    private static function skipUntilSemicolon(string $src, int $i): int
    {
        $len = strlen($src);
        while ($i < $len && $src[$i] !== ';') {
            $skipped = self::skipNonCode($src, $i);
            $i = $skipped === $i ? $i + 1 : $skipped;
        }

        return $i;
    }

    /** Skips to the first opening delimiter at or after $i, then past its matching close. */
    private static function skipDelimited(string $src, int $i): int
    {
        $len = strlen($src);
        while ($i < $len && !in_array($src[$i], ['{', '(', '['], true)) {
            $i++;
        }

        $depth = 0;
        while ($i < $len) {
            $skipped = self::skipNonCode($src, $i);
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
