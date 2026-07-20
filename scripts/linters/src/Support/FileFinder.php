<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

final class FileFinder
{
    // Returns absolute paths to each crate's Cargo.toml (crates/*/Cargo.toml), sorted.
    public static function cargoTomls(string $rootDir): array
    {
        $paths = glob("{$rootDir}/crates/*/Cargo.toml") ?: [];
        sort($paths);

        return $paths;
    }

    /** @return list<string> absolute paths to *.rs files under crates/, sorted */
    public static function rustFiles(string $rootDir): array
    {
        return self::walk("{$rootDir}/crates", static fn (string $path): bool => str_ends_with($path, '.rs'));
    }

    // Returns absolute paths to mod.rs files under each crate's src/ tree
    // (crates/*/src/**/mod.rs), sorted.
    public static function modRsFiles(string $rootDir): array
    {
        $found = [];

        foreach (glob("{$rootDir}/crates/*", GLOB_ONLYDIR) ?: [] as $crateDir) {
            $srcDir = "{$crateDir}/src";
            if (!is_dir($srcDir)) {
                continue;
            }

            foreach (self::walk($srcDir, static fn (string $path): bool => basename($path) === 'mod.rs') as $path) {
                $found[] = $path;
            }
        }

        sort($found);

        return $found;
    }

    /** @return list<string> */
    private static function walk(string $baseDir, callable $predicate): array
    {
        if (!is_dir($baseDir)) {
            return [];
        }

        $found = [];
        $iterator = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($baseDir, \FilesystemIterator::SKIP_DOTS),
        );
        foreach ($iterator as $file) {
            /** @var \SplFileInfo $file */
            if ($file->isFile() && $predicate($file->getPathname())) {
                $found[] = $file->getPathname();
            }
        }

        sort($found);

        return $found;
    }
}
