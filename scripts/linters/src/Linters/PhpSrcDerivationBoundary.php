<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class PhpSrcDerivationBoundary implements Linter
{
    private const PHP_SRC_CRATE_PREFIX = 'crates/shirabe-php-src/';

    // The origin marker every php-src derived item carries, plus the bare crate name.
    private const MARKER_RE = '/php-src/i';

    public function name(): string
    {
        return 'php_src_derivation_boundary';
    }

    public function failureIntro(): string
    {
        return "Found `php-src` mentioned outside the `shirabe-php-src` crate.\n"
            . "Code written by reading php-src is licensed under php-src's terms, not Shirabe's MIT,\n"
            . 'so it must live in `crates/shirabe-php-src/` and be reached through that crate:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (str_starts_with($relative, self::PHP_SRC_CRATE_PREFIX)) {
                continue;
            }
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (file($path) as $idx => $raw) {
                if (!preg_match(self::MARKER_RE, $raw)) {
                    continue;
                }

                $errors[] = "{$relative}:" . ($idx + 1) . ': ' . trim($raw);
            }
        }

        return $errors;
    }
}
