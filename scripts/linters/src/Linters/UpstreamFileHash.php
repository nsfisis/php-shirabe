<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\RefHeader;

final class UpstreamFileHash implements Linter
{
    public function name(): string
    {
        return 'upstream_file_hash';
    }

    public function failureIntro(): string
    {
        return "Found `//! ref: <file> sha1:<hash>` headers that do not match the sources under\n"
            . "composer/. On a hash mismatch, the file changed upstream: check that the Rust port\n"
            . "follows it, then run `scripts/linters/upstream-file-hash update <rust-file>...`:";
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (file($path) as $idx => $raw) {
                if (!preg_match(RefHeader::CANDIDATE_RE, $raw)) {
                    continue;
                }
                $location = "{$relative}:" . ($idx + 1);
                $line = rtrim($raw, "\r\n");

                if (!preg_match(RefHeader::HEADER_RE, $line, $m)) {
                    $errors[] = "{$location}: malformed header, or no hash: {$line}";
                    continue;
                }
                [, $file, $recorded] = $m;

                try {
                    $actual = RefHeader::hash($rootDir, $file);
                } catch (\RuntimeException $e) {
                    $errors[] = "{$location}: {$e->getMessage()}";
                    continue;
                }

                if ($actual !== $recorded) {
                    $errors[] = "{$location}: {$file} changed: recorded sha1:{$recorded}, current sha1:{$actual}";
                }
            }
        }

        return $errors;
    }
}
