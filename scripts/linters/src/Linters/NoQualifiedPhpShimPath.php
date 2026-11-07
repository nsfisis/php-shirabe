<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\QualifiedPathScanner;

/**
 * Items of `shirabe_php_shim` stand for PHP built-ins, which PHP code calls unqualified, so they
 * are imported with `use` rather than spelled out at every call site. Macros are exempt: the body
 * of a `macro_rules!` expands in the caller's scope and cannot rely on its imports, and a macro is
 * invoked through its path.
 */
final class NoQualifiedPhpShimPath implements Linter
{
    public function name(): string
    {
        return 'no_qualified_php_shim_path';
    }

    public function failureIntro(): string
    {
        return "Found `shirabe_php_shim::` paths outside `use` statements.\n"
            . 'Import the item with `use shirabe_php_shim::...;` instead '
            . '(macro invocations and `macro_rules!` bodies are exempt):';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findQualifiedPaths(file_get_contents($path), $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findQualifiedPaths(string $src, string $relative): array
    {
        $errors = [];
        foreach (QualifiedPathScanner::find($src, 'shirabe_php_shim') as $found) {
            $first = explode('::', $found['path'])[1];
            $errors[] = "{$relative}:{$found['line']}: `shirabe_php_shim::{$first}` (import it with `use`)";
        }

        return $errors;
    }
}
