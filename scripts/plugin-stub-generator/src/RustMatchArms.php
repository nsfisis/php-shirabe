<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/**
 * Reads the byte-string patterns of the `match` arms in one Rust function, the way the RPC
 * dispatchers spell the members they serve: `b"getName" | b"getPrettyName" => ...` for a method
 * name, `(b"Composer\\Util\\Filesystem", b"isLocalPath") => ...` for a class and a method.
 *
 * A token-level reader rather than a Rust parser: it skips comments and literals, balances
 * brackets, and splits the arms of a match block at its top level. An arm alternative it cannot
 * read as one of the two shapes above fails the run instead of being skipped, so a dispatcher
 * written in another style surfaces here rather than silently dropping out of the manifest.
 */
final class RustMatchArms
{
    private const BLOCK_LIKE = ['match', 'if', 'loop', 'unsafe', 'while', 'for'];

    /** @var list<array{0: string, 1: string}> kind and text; `bstr` texts are decoded */
    private array $tokens = [];

    private function __construct(private readonly string $path, string $code)
    {
        $this->tokens = RustTokens::tokenize($path, $code);
    }

    /**
     * Finds the one file under `$sourceDir` that defines `fn $function` and reads its arms.
     *
     * @param string $scrutinee the scrutinee of the matches to read, tokens joined without spaces
     *                          (e.g. `method_name.as_bytes()`); a match on a tuple whose first
     *                          element it is counts too, and is read as that element
     * @return list<list<?string>> one entry per arm alternative: its byte-string literal, or
     *                             one per element of its tuple (null for an element that is
     *                             not a byte-string literal)
     */
    public static function inFunction(string $sourceDir, string $function, string $scrutinee): array
    {
        $files = [];
        $iterator = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($sourceDir, \FilesystemIterator::SKIP_DOTS)
        );
        foreach ($iterator as $entry) {
            if (!$entry->isFile() || !str_ends_with($entry->getFilename(), '.rs')) {
                continue;
            }
            $code = file_get_contents($entry->getPathname());
            if (preg_match('/\bfn\s+' . preg_quote($function, '/') . '\s*[<(]/', $code)) {
                $files[$entry->getPathname()] = $code;
            }
        }
        ksort($files);
        if (count($files) !== 1) {
            throw new GenerationError([
                "expected exactly one definition of fn $function under $sourceDir, found "
                    . (count($files) === 0 ? 'none' : implode(', ', array_keys($files))),
            ]);
        }
        $path = array_key_first($files);
        $reader = new self($path, $files[$path]);

        return $reader->armsOf($function, $scrutinee);
    }

    /** @return list<list<?string>> */
    private function armsOf(string $function, string $scrutinee): array
    {
        [$bodyStart, $bodyEnd] = $this->functionBody($function);
        $alternatives = [];
        $found = false;
        for ($i = $bodyStart + 1; $i < $bodyEnd; $i++) {
            if ($this->tokens[$i] !== ['ident', 'match']) {
                continue;
            }
            $text = '';
            $j = $i + 1;
            while ($j < $bodyEnd && $this->tokens[$j][1] !== '{') {
                $text .= $this->tokens[$j][1];
                $j++;
            }
            $first = str_starts_with($text, '(' . $scrutinee . ',');
            if ($text !== $scrutinee && !$first) {
                continue;
            }
            $found = true;
            $blockEnd = $this->closing($j);
            foreach ($this->armsIn($j, $blockEnd) as $alternative) {
                if (!$first) {
                    $alternatives[] = $alternative;
                } elseif ($alternative[0] !== null) {
                    $alternatives[] = [$alternative[0]];
                }
            }
        }
        if (!$found) {
            throw new GenerationError(["{$this->path}: fn $function has no `match $scrutinee`"]);
        }

        return $alternatives;
    }

    /** @return array{0: int, 1: int} the indices of the body's braces */
    private function functionBody(string $function): array
    {
        $count = count($this->tokens);
        for ($i = 0; $i + 1 < $count; $i++) {
            if ($this->tokens[$i] === ['ident', 'fn'] && $this->tokens[$i + 1] === ['ident', $function]) {
                for ($j = $i + 2; $j < $count; $j++) {
                    if ($this->tokens[$j][1] === ';') {
                        break;
                    }
                    if ($this->tokens[$j][1] === '{') {
                        return [$j, $this->closing($j)];
                    }
                }
            }
        }
        throw new GenerationError(["{$this->path}: cannot find the body of fn $function"]);
    }

    /** The index of the bracket closing the one at `$open`. */
    private function closing(int $open): int
    {
        $depth = 0;
        $count = count($this->tokens);
        for ($i = $open; $i < $count; $i++) {
            $text = $this->tokens[$i][1];
            if ($this->tokens[$i][0] !== 'punct') {
                continue;
            }
            if ($text === '(' || $text === '[' || $text === '{') {
                $depth++;
            } elseif ($text === ')' || $text === ']' || $text === '}') {
                $depth--;
                if ($depth === 0) {
                    return $i;
                }
            }
        }
        throw new GenerationError(["{$this->path}: unbalanced brackets"]);
    }

    /**
     * Splits the arms of the match block between the braces at `$open` and `$close`.
     *
     * @return list<list<?string>>
     */
    private function armsIn(int $open, int $close): array
    {
        $alternatives = [];
        $i = $open + 1;
        while ($i < $close) {
            $patternStart = $i;
            $guard = null;
            while ($i < $close && !$this->is($i, '=>')) {
                if ($this->isOpening($i)) {
                    $i = $this->closing($i);
                } elseif ($guard === null && $this->tokens[$i] === ['ident', 'if']) {
                    $guard = $i;
                }
                $i++;
            }
            if ($i >= $close) {
                throw new GenerationError(["{$this->path}: a match arm without `=>`"]);
            }
            foreach ($this->alternatives($patternStart, $guard ?? $i) as $alternative) {
                $alternatives[] = $alternative;
            }
            $i = $this->skipArmBody($i + 1, $close);
        }

        return $alternatives;
    }

    /** @return list<list<?string>> the readable alternatives of the pattern in `[$start, $end)` */
    private function alternatives(int $start, int $end): array
    {
        $alternatives = [];
        $current = [];
        for ($i = $start; $i <= $end; $i++) {
            if ($i === $end || $this->is($i, '|')) {
                $alternative = $this->readAlternative($current);
                if ($alternative !== null) {
                    $alternatives[] = $alternative;
                }
                $current = [];
                continue;
            }
            if ($this->isOpening($i)) {
                $closing = $this->closing($i);
                for ($j = $i; $j <= $closing; $j++) {
                    $current[] = $this->tokens[$j];
                }
                $i = $closing;
                continue;
            }
            $current[] = $this->tokens[$i];
        }

        return $alternatives;
    }

    /**
     * @param list<array{0: string, 1: string}> $tokens
     * @return list<?string>|null null for an alternative that names no member (`other`, `_`)
     */
    private function readAlternative(array $tokens): ?array
    {
        if (!in_array('bstr', array_column($tokens, 0), true)) {
            return null;
        }
        if (count($tokens) === 1) {
            return [$tokens[0][1]];
        }
        $count = count($tokens);
        if ($tokens[0] !== ['punct', '('] || $tokens[$count - 1] !== ['punct', ')']) {
            $this->unrecognized($tokens);
        }
        $elements = [];
        $element = [];
        $depth = 0;
        for ($i = 1; $i < $count; $i++) {
            $token = $tokens[$i];
            if ($depth === 0 && ($token === ['punct', ','] || $i === $count - 1)) {
                if (!in_array('bstr', array_column($element, 0), true)) {
                    $elements[] = null;
                } elseif (count($element) === 1) {
                    $elements[] = $element[0][1];
                } else {
                    $this->unrecognized($tokens);
                }
                $element = [];
                continue;
            }
            if ($token[0] === 'punct' && in_array($token[1], ['(', '[', '{'], true)) {
                $depth++;
            } elseif ($token[0] === 'punct' && in_array($token[1], [')', ']', '}'], true)) {
                $depth--;
            }
            $element[] = $token;
        }

        return $elements;
    }

    /** @param list<array{0: string, 1: string}> $tokens */
    private function unrecognized(array $tokens): never
    {
        throw new GenerationError([
            "{$this->path}: unrecognized match arm pattern `"
                . implode(' ', array_column($tokens, 1)) . '`',
        ]);
    }

    /** Skips the body of an arm starting at `$i`; answers the index of the next arm. */
    private function skipArmBody(int $i, int $close): int
    {
        $blockLike = $i < $close
            && ($this->is($i, '{')
                || ($this->tokens[$i][0] === 'ident' && in_array($this->tokens[$i][1], self::BLOCK_LIKE, true)));
        while ($i < $close) {
            if ($this->is($i, ',')) {
                return $i + 1;
            }
            if ($this->isOpening($i)) {
                $closing = $this->closing($i);
                $i = $closing + 1;
                if ($this->is($closing, '}') && $blockLike && $i < $close) {
                    if ($this->is($i, ',')) {
                        return $i + 1;
                    }
                    // A block-like body needs no comma, so whatever cannot continue the
                    // expression starts the next arm.
                    $next = $this->tokens[$i];
                    $continues = ($next[0] === 'ident' && in_array($next[1], ['else', 'as'], true))
                        || ($next[0] === 'punct' && !in_array($next[1], ['(', '['], true));
                    if (!$continues) {
                        return $i;
                    }
                }
                continue;
            }
            $i++;
        }

        return $close;
    }

    private function is(int $i, string $punct): bool
    {
        return $this->tokens[$i][0] === 'punct' && $this->tokens[$i][1] === $punct;
    }

    private function isOpening(int $i): bool
    {
        return $this->tokens[$i][0] === 'punct' && in_array($this->tokens[$i][1], ['(', '[', '{'], true);
    }
}
