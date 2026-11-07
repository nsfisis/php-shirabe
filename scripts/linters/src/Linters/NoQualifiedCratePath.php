<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\QualifiedPathScanner;

/**
 * Items of the current crate, macros included, are imported with `use crate::...;` rather than
 * spelled out as `crate::...` at the use site; a name that collides with another import is renamed
 * with `as`. The body of a `macro_rules!` is exempt: it expands in the caller's scope and cannot
 * rely on its imports. Restricted visibilities (`pub(in crate::...)`) are exempt too, as they are
 * not item references.
 */
final class NoQualifiedCratePath implements Linter
{
    public function name(): string
    {
        return 'no_qualified_crate_path';
    }

    public function failureIntro(): string
    {
        return "Found `crate::` paths outside `use` statements.\n"
            . 'Import the item with `use crate::...;` instead, renaming it with `as` on a collision '
            . '(`macro_rules!` bodies are exempt):';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (QualifiedPathScanner::find(file_get_contents($path), 'crate') as $found) {
                $errors[] = "{$relative}:{$found['line']}: `{$found['path']}` (import it with `use`)";
            }
        }

        return $errors;
    }
}
