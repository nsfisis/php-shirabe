<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

final class UseTree
{
    /**
     * Flattens a use tree (the text between `use` and `;`) into one path per imported item.
     * A trailing `as name` stays on the path it renames.
     *
     * @return list<string>
     */
    public static function expand(string $tree): array
    {
        $tree = trim($tree);
        $brace = strpos($tree, '{');

        if ($brace === false) {
            return $tree === '' ? [] : [$tree];
        }

        $prefix = trim(preg_replace('/::\s*\z/', '', substr($tree, 0, $brace)));
        $inner = preg_replace('/\}\s*\z/', '', substr($tree, $brace + 1));

        $result = [];
        foreach (self::splitTopLevel($inner) as $child) {
            foreach (self::expand($child) as $sub) {
                if ($sub === 'self') {
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

    /** @return list<string> */
    private static function splitTopLevel(string $str): array
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
