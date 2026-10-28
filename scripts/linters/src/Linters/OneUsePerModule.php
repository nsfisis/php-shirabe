<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\UseTree;

/**
 * Items imported from the same module go in one `use` statement (rustfmt's
 * `imports_granularity = "Module"`, which stable rustfmt cannot apply). Statements are compared
 * only within one run of `use` statements, and only when they share the indentation and the
 * visibility. A statement carrying an attribute or a comment, or importing from more than one
 * module, is left alone.
 */
final class OneUsePerModule implements Linter
{
    private const USE_RE = '/\A(\s*)((?:pub(?:\([^)]*\))?\s+)?)use\s+(.*)\z/s';

    public function name(): string
    {
        return 'one_use_per_module';
    }

    public function failureIntro(): string
    {
        return "Found several `use` statements importing from the same module.\n"
            . 'Merge them into one, e.g. `use foo::bar::{baz, qux};`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findSplitModules(file($path), $relative));
        }

        return $errors;
    }

    /**
     * @param list<string> $lines
     * @return list<string>
     */
    private function findSplitModules(array $lines, string $relative): array
    {
        $errors = [];
        $seen = [];
        $block = 0;
        $count = count($lines);
        $i = 0;

        while ($i < $count) {
            $stripped = trim($lines[$i]);
            if (str_starts_with($stripped, '//') || !preg_match(self::USE_RE, rtrim($lines[$i], "\n"), $m)) {
                if ($stripped !== '' && !str_starts_with($stripped, '//')) {
                    $block++;
                }
                $i++;
                continue;
            }

            $start = $i;
            $text = $lines[$i];
            while (!str_ends_with(rtrim($text), ';') && $i + 1 < $count) {
                $i++;
                $text .= $lines[$i];
            }
            $i++;

            if (str_contains($text, '//') || str_contains($text, '/*')) {
                continue;
            }
            if ($start > 0 && str_starts_with(trim($lines[$start - 1]), '#[')) {
                continue;
            }

            preg_match(self::USE_RE, rtrim($text), $m);
            $modules = [];
            foreach (UseTree::expand(preg_replace('/;\s*\z/', '', $m[3])) as $path) {
                $path = preg_replace('/\s+as\s+\S+\z/', '', $path);
                $separator = strrpos($path, '::');
                $modules[$separator === false ? '' : substr($path, 0, $separator)] = true;
            }
            if (count($modules) !== 1 || isset($modules[''])) {
                continue;
            }

            $module = array_key_first($modules);
            $key = "{$block}\0{$m[1]}\0" . trim($m[2]) . "\0{$module}";
            if (isset($seen[$key])) {
                $errors[] = "{$relative}:" . ($start + 1) . ": `{$module}` is already imported from on line {$seen[$key]}";
            } else {
                $seen[$key] = $start + 1;
            }
        }

        return $errors;
    }
}
