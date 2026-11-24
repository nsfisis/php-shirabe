<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

/**
 * An exception crosses the boundary to the PHP worker through one conversion in each direction,
 * so that a `catch` on either side sees the class the exception was thrown as:
 *
 * - Rust to PHP: `error_throw` (`crates/shirabe/src/php_worker/rpc/dispatch.rs`), which keeps the
 *   ported exception an `anyhow::Error` carries.
 * - PHP to Rust: `throw_into_error` (`crates/shirabe/src/php_worker/throwable.rs`), reached from
 *   `shirabe-php-rpc` through `PhpThrow::into_error`.
 *
 * A `PhpThrow` built from the message of an error (`error_chain_message`) flattens the exception
 * into a `RuntimeException`, and a `std::error::Error` impl on `PhpThrow` lets `?` carry it into an
 * `anyhow::Error` that no `catch` recognizes.
 */
final class PhpThrowConversion implements Linter
{
    private const SCOPES = [
        'crates/shirabe/src/php_worker/',
        'crates/shirabe/src/php_worker.rs',
        'crates/shirabe-php-rpc/src/',
    ];

    private const PATTERNS = [
        '/\berror_chain_message\b/',
        '/\bimpl\s+(?:std::error::)?Error\s+for\s+PhpThrow\b/',
    ];

    public function name(): string
    {
        return 'php_throw_conversion';
    }

    public function failureIntro(): string
    {
        return (
            "Found a conversion between `PhpThrow` and `anyhow::Error` outside the canonical ones.\n"
            . 'Use `error_throw` (Rust to PHP) or `throw_into_error` / `PhpThrow::into_error` (PHP to Rust):'
        );
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];
        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (!$this->inScope($relative) || in_array($relative, $excludes, true)) {
                continue;
            }
            foreach (file($path) as $idx => $raw) {
                $code = explode('//', $raw, 2)[0];
                foreach (self::PATTERNS as $pattern) {
                    if (preg_match($pattern, $code)) {
                        $errors[] = "{$relative}:" . ($idx + 1) . ': ' . trim($raw);
                        break;
                    }
                }
            }
        }

        return $errors;
    }

    private function inScope(string $relative): bool
    {
        foreach (self::SCOPES as $scope) {
            if (str_starts_with($relative, $scope)) {
                return true;
            }
        }

        return false;
    }
}
