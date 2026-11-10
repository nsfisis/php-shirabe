<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * The plugin class classifier exempts the classes on its constructible list from the
 * dual-instantiation demotion, so that list has to name exactly the classes the Rust side builds
 * an entity for: the `constructor supported` rows of the plugin surface manifest.
 */
final class PluginConstructibleList implements Linter
{
    private const MANIFEST = 'crates/shirabe/src/php_worker/rpc/surface.tsv';
    private const LIST = 'scripts/plugin-class-classifier/lists/constructible.list';

    public function name(): string
    {
        return 'plugin_constructible_list';
    }

    public function failureIntro(): string
    {
        return 'Found a mismatch between ' . self::LIST . " and the classes\n"
            . 'the constructors ' . self::MANIFEST . ' records as supported:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        $built = [];
        foreach ($this->supportedConstructors("{$rootDir}/" . self::MANIFEST, $errors) as $fqcn) {
            $built[$fqcn] = true;
        }

        $listed = [];
        foreach ($this->listEntries("{$rootDir}/" . self::LIST, $errors) as $fqcn) {
            if (isset($listed[$fqcn])) {
                $errors[] = self::LIST . ": `{$fqcn}` is listed more than once";
            }
            $listed[$fqcn] = true;
        }

        foreach (array_keys($built) as $fqcn) {
            if (!isset($listed[$fqcn])) {
                $errors[] = self::LIST . ": `{$fqcn}` is constructible but not listed";
            }
        }
        foreach (array_keys($listed) as $fqcn) {
            if (!isset($built[$fqcn])) {
                $errors[] = self::LIST . ": `{$fqcn}` is listed but its constructor is not supported";
            }
        }

        return $errors;
    }

    /**
     * @param list<string> $errors
     * @return list<string>
     */
    private function supportedConstructors(string $path, array &$errors): array
    {
        $lines = @file($path, FILE_IGNORE_NEW_LINES);
        if ($lines === false) {
            $errors[] = self::MANIFEST . ': cannot be read';

            return [];
        }

        $classes = [];
        foreach ($lines as $line) {
            if (str_starts_with($line, '#')) {
                continue;
            }
            $fields = explode("\t", $line);
            if (count($fields) === 3 && $fields[1] === 'constructor' && $fields[2] === 'supported') {
                $classes[] = substr($fields[0], 0, -strlen('::__construct'));
            }
        }

        return $classes;
    }

    /**
     * @param list<string> $errors
     * @return list<string>
     */
    private function listEntries(string $path, array &$errors): array
    {
        $lines = @file($path, FILE_IGNORE_NEW_LINES);
        if ($lines === false) {
            $errors[] = self::LIST . ': cannot be read';

            return [];
        }

        $entries = [];
        foreach ($lines as $line) {
            $line = trim($line);
            if ($line === '' || str_starts_with($line, '#')) {
                continue;
            }
            $entries[] = $line;
        }

        return $entries;
    }
}
