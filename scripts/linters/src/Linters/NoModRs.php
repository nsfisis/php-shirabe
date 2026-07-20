<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoModRs implements Linter
{
    public function name(): string
    {
        return 'no_mod_rs';
    }

    public function failureIntro(): string
    {
        return 'Found `mod.rs` file(s). Use `src/<submodule>.rs` instead of `<submodule>/mod.rs`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::modRsFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            $errors[] = $relative;
        }

        return $errors;
    }
}
