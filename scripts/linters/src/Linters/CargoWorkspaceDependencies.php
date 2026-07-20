<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class CargoWorkspaceDependencies implements Linter
{
    private const SECTION_NAMES = ['dependencies', 'dev-dependencies', 'build-dependencies'];

    public function name(): string
    {
        return 'cargo_workspace_dependencies';
    }

    public function failureIntro(): string
    {
        return "Found `[dependencies]` / `[dev-dependencies]` entries that do not use `workspace = true`.\n"
            . 'In a crate `Cargo.toml`, only `name.workspace = true` or `name = { workspace = true, ... }` is allowed:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::cargoTomls($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findNonWorkspaceDeps($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findNonWorkspaceDeps(string $path, string $relative): array
    {
        $errors = [];
        $currentSection = null;

        foreach (file($path) as $idx => $rawLine) {
            $stripped = trim($rawLine);

            if (preg_match('/\A\[([^\]]+)\]\z/', $stripped, $m)) {
                $currentSection = $m[1];
                continue;
            }

            if ($currentSection === null || !in_array($currentSection, self::SECTION_NAMES, true)) {
                continue;
            }
            if ($stripped === '' || str_starts_with($stripped, '#')) {
                continue;
            }

            if (preg_match('/\A([A-Za-z0-9_-]+)\.workspace\s*=\s*true\b/', $stripped)) {
                continue;
            }

            if (preg_match('/\A([A-Za-z0-9_-]+)\s*=\s*\{(.+)\}\s*\z/', $stripped, $m)) {
                [, $name, $inner] = $m;
                if (preg_match('/\bworkspace\s*=\s*true\b/', $inner)) {
                    continue;
                }
                $errors[] = "{$relative}:" . ($idx + 1) . ": `{$name}` does not use `workspace = true`";
                continue;
            }

            if (preg_match('/\A([A-Za-z0-9_-]+)\s*=/', $stripped, $m)) {
                $errors[] = "{$relative}:" . ($idx + 1) . ": `{$m[1]}` does not use `workspace = true`";
            }
        }

        return $errors;
    }
}
