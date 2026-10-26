<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoToStringLossy implements Linter
{
    public function name(): string
    {
        return 'no_to_string_lossy';
    }

    public function failureIntro(): string
    {
        return "Found calls to `to_string_lossy()`.\n"
            . "It irreversibly replaces bytes that are not valid UTF-8, which PHP never does.\n"
            . "To display a path, use `display()`. To keep processing it as a string, keep the\n"
            . 'bytes (e.g. `as_encoded_bytes()`, `bstr`) or fail loudly on invalid UTF-8:';
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
                $code = explode('//', $raw, 2)[0];
                if (!preg_match('/\bto_string_lossy\b/', $code)) {
                    continue;
                }

                $errors[] = "{$relative}:" . ($idx + 1) . ': ' . trim($raw);
            }
        }

        return $errors;
    }
}
