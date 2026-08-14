<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoHaltCompilerLiteral implements Linter
{
    // Assembled so that this file does not contain what it looks for.
    private const TOKEN = '__' . 'HALT_COMPILER();';

    public function name(): string
    {
        return 'no_halt_compiler_literal';
    }

    public function failureIntro(): string
    {
        return "Found a literal `" . self::TOKEN . "` (case-insensitive).\n"
            . "The executable carries the Composer runtime bundle as a phar that PHP finds by\n"
            . "scanning for the first occurrence of that token, and every literal here ends up in\n"
            . "the same binary, so an earlier one shadows the bundle. Build the token at run time\n"
            . "instead — see "
            . '`halt_compiler_token` in `crates/shirabe-php-shim/src/phar.rs`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach ($this->embeddedFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (file($path) as $idx => $raw) {
                if (stripos($raw, self::TOKEN) === false) {
                    continue;
                }

                $errors[] = "{$relative}:" . ($idx + 1) . ': ' . trim($raw);
            }
        }

        return $errors;
    }

    /**
     * The sources whose bytes reach the binary: Rust code, and the PHP files the Rust code
     * embeds with `include_str!`.
     *
     * @return list<string>
     */
    private function embeddedFiles(string $rootDir): array
    {
        $paths = FileFinder::rustFiles($rootDir);

        foreach (glob("{$rootDir}/crates/*/php", GLOB_ONLYDIR) ?: [] as $phpDir) {
            $iterator = new \RecursiveIteratorIterator(
                new \RecursiveDirectoryIterator($phpDir, \FilesystemIterator::SKIP_DOTS),
            );
            foreach ($iterator as $file) {
                /** @var \SplFileInfo $file */
                if ($file->isFile()) {
                    $paths[] = $file->getPathname();
                }
            }
        }

        sort($paths);

        return $paths;
    }
}
