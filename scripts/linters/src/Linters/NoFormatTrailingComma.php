<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoFormatTrailingComma implements Linter
{
    public function name(): string
    {
        return 'no_format_trailing_comma';
    }

    public function failureIntro(): string
    {
        return 'Found `,)` introduced by formatting. Remove it.';
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
                if (!str_contains($raw, ',)')) {
                    continue;
                }
                // `(,)` is a macro repetition fragment (e.g. `$(,)?`).
                if (str_contains($raw, '(,)')) {
                    continue;
                }

                $errors[] = "{$relative}:" . ($idx + 1) . ': trailing `,)` before a closing paren';
            }
        }

        return $errors;
    }
}
