<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\RefHeader;

/**
 * Every `include_str!` / `include_bytes!` of a file under composer/ carries a `// ref:` comment
 * naming that file, so that a change upstream shows up as a hash mismatch of `upstream_file_hash`
 * even though the embedded bytes follow it on their own. Conversely, every `// ref:` comment
 * stands next to an embedding of the file it names.
 *
 * "Next to" means within the statement or item the macro is part of: the lines above the macro,
 * up to the nearest blank line or the nearest code line ending with `;`, `{` or `}`.
 */
final class UpstreamIncludeRef implements Linter
{
    private const INCLUDE_RE = '/\binclude_(?:str|bytes)!\(\s*"((?:[^"\\\\]|\\\\.)*)"\s*\)/';

    public function name(): string
    {
        return 'upstream_include_ref';
    }

    public function failureIntro(): string
    {
        return "Found an `include_str!` / `include_bytes!` of a file under composer/ without a\n"
            . "`// ref: <file> sha1:<hash>` comment in the statement or item that embeds it, or a\n"
            . "`// ref:` comment that no such embedding follows. Write the comment above the\n"
            . "embedding with the path of the file, then run\n"
            . "`scripts/linters/upstream-file-hash update <file>...`:";
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            $errors = [...$errors, ...$this->checkFile($rootDir, $path, $relative)];
        }

        return $errors;
    }

    /** @return list<string> */
    private function checkFile(string $rootDir, string $path, string $relative): array
    {
        $source = file_get_contents($path);
        $lines = explode("\n", $source);

        // Line index => the file named by a `// ref:` comment on it.
        $refs = [];
        foreach ($lines as $idx => $line) {
            if (
                preg_match(RefHeader::CANDIDATE_RE, $line)
                && preg_match(RefHeader::UPDATABLE_RE, $line, $m)
                && $m[1] !== '//!'
            ) {
                $refs[$idx] = $m[2];
            }
        }

        $errors = [];
        $used = [];

        preg_match_all(self::INCLUDE_RE, $source, $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
        foreach ($matches as $match) {
            [$macro, $offset] = $match[0];
            $file = $this->upstreamPath($rootDir, dirname($path), stripcslashes($match[1][0]));
            if ($file === null) {
                continue;
            }

            $idx = substr_count($source, "\n", 0, $offset);
            $found = false;
            foreach ($this->statementLines($lines, $idx) as $candidate) {
                if (($refs[$candidate] ?? null) === $file) {
                    $used[$candidate] = true;
                    $found = true;
                }
            }

            if (!$found) {
                $errors[] = "{$relative}:" . ($idx + 1) . ": no `// ref: {$file}` for {$macro}";
            }
        }

        foreach ($refs as $idx => $file) {
            if (!isset($used[$idx])) {
                $errors[] = "{$relative}:" . ($idx + 1) . ": no embedding of {$file} follows: " . trim($lines[$idx]);
            }
        }

        return $errors;
    }

    /**
     * The indices of the lines above line `$idx` that belong to the same statement or item.
     *
     * @param list<string> $lines
     * @return list<int>
     */
    private function statementLines(array $lines, int $idx): array
    {
        $found = [];

        for ($i = $idx - 1; $i >= 0; $i--) {
            $line = trim($lines[$i]);
            if ($line === '') {
                break;
            }
            if (!str_starts_with($line, '//') && preg_match('/[;{}]$/', $line)) {
                break;
            }
            $found[] = $i;
        }

        return $found;
    }

    /**
     * The path relative to `$rootDir` of the file `$literal` names, relative to `$dir`, when it is
     * under composer/.
     */
    private function upstreamPath(string $rootDir, string $dir, string $literal): ?string
    {
        $parts = [];
        foreach (explode('/', "{$dir}/{$literal}") as $part) {
            if ($part === '' || $part === '.') {
                continue;
            }
            if ($part === '..') {
                array_pop($parts);
                continue;
            }
            $parts[] = $part;
        }

        $relative = Paths::relativeTo($rootDir, '/' . implode('/', $parts));

        return str_starts_with($relative, RefHeader::SOURCE_PREFIX) ? $relative : null;
    }
}
