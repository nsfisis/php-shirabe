<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/**
 * A token-level reader of Rust source: comments are skipped, byte-string literals are decoded,
 * and every other literal is kept as one opaque token.
 */
final class RustTokens
{
    /** @return list<array{0: string, 1: string}> */
    public static function tokenize(string $path, string $code): array
    {
        $tokens = [];
        $length = strlen($code);
        $offset = 0;
        $patterns = [
            'space' => '\s+',
            'comment' => '\/\/[^\n]*',
            'block' => '\/\*',
            'rawstr' => 'b?r(#*)"',
            'bstr' => 'b"((?:[^"\\\\]|\\\\.)*)"',
            'str' => '"(?:[^"\\\\]|\\\\.)*"',
            'bchar' => "b'(?:[^'\\\\]|\\\\.)[^']*'",
            'char' => "'(?:[^'\\\\\\n]|\\\\[^\\n]+?)'",
            'lifetime' => "'[A-Za-z_][A-Za-z0-9_]*",
            'ident' => '(?:r#)?[A-Za-z_][A-Za-z0-9_]*',
            'number' => '[0-9][0-9A-Za-z_.]*',
            'punct' => '=>|::|->|\.\.=?|&&|\|\||[=!<>]=|[^\s]',
        ];
        while ($offset < $length) {
            $matched = false;
            foreach ($patterns as $kind => $pattern) {
                if (!preg_match('/\G' . $pattern . '/su', $code, $m, 0, $offset)) {
                    continue;
                }
                $matched = true;
                if ($kind === 'block') {
                    $offset = self::skipBlockComment($path, $code, $offset);
                    break;
                }
                if ($kind === 'rawstr') {
                    $terminator = '"' . $m[1];
                    $end = strpos($code, $terminator, $offset + strlen($m[0]));
                    if ($end === false) {
                        throw new GenerationError(["$path: unterminated raw string"]);
                    }
                    $tokens[] = ['str', substr($code, $offset, $end + strlen($terminator) - $offset)];
                    $offset = $end + strlen($terminator);
                    break;
                }
                $offset += strlen($m[0]);
                if ($kind === 'space' || $kind === 'comment') {
                    break;
                }
                if ($kind === 'bstr') {
                    $tokens[] = ['bstr', stripcslashes($m[1])];
                } elseif ($kind === 'bchar' || $kind === 'char' || $kind === 'number') {
                    $tokens[] = ['literal', $m[0]];
                } else {
                    $tokens[] = [$kind === 'lifetime' ? 'lifetime' : $kind, $m[0]];
                }
                break;
            }
            if (!$matched) {
                throw new GenerationError(["$path: cannot tokenize at byte $offset"]);
            }
        }

        return $tokens;
    }

    private static function skipBlockComment(string $path, string $code, int $offset): int
    {
        $depth = 0;
        $length = strlen($code);
        while ($offset < $length) {
            if (substr($code, $offset, 2) === '/*') {
                $depth++;
                $offset += 2;
            } elseif (substr($code, $offset, 2) === '*/') {
                $depth--;
                $offset += 2;
                if ($depth === 0) {
                    return $offset;
                }
            } else {
                $offset++;
            }
        }
        throw new GenerationError(["$path: unterminated block comment"]);
    }
}
