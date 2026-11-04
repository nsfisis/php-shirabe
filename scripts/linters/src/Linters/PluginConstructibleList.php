<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * The plugin class classifier exempts the classes on its constructible list from the
 * dual-instantiation demotion, so that list has to name exactly the classes `construct_entity`
 * builds: one per `b"FQCN" =>` arm of its body.
 */
final class PluginConstructibleList implements Linter
{
    private const DISPATCH = 'crates/shirabe/src/plugin/rpc/dispatch.rs';
    private const LIST = 'scripts/plugin-class-classifier/lists/constructible.list';

    public function name(): string
    {
        return 'plugin_constructible_list';
    }

    public function failureIntro(): string
    {
        return 'Found a mismatch between ' . self::LIST . " and the classes\n"
            . '`construct_entity` in ' . self::DISPATCH . ' builds:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        $built = [];
        foreach ($this->constructEntityArms("{$rootDir}/" . self::DISPATCH, $errors) as $fqcn) {
            if (isset($built[$fqcn])) {
                $errors[] = self::DISPATCH . ": `{$fqcn}` has more than one arm in `construct_entity`";
            }
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
                $errors[] = self::LIST . ": `{$fqcn}` is built by `construct_entity` but not listed";
            }
        }
        foreach (array_keys($listed) as $fqcn) {
            if (!isset($built[$fqcn])) {
                $errors[] = self::LIST . ": `{$fqcn}` is listed but `construct_entity` does not build it";
            }
        }

        return $errors;
    }

    /**
     * @param list<string> $errors
     * @return list<string>
     */
    private function constructEntityArms(string $path, array &$errors): array
    {
        $lines = @file($path, FILE_IGNORE_NEW_LINES);
        if ($lines === false) {
            $errors[] = self::DISPATCH . ': cannot be read';

            return [];
        }

        $start = null;
        foreach ($lines as $idx => $line) {
            if (preg_match('/^pub\(crate\) fn construct_entity\(/', $line)) {
                $start = $idx;
                break;
            }
        }
        if ($start === null) {
            $errors[] = self::DISPATCH . ': no top-level `pub(crate) fn construct_entity(`';

            return [];
        }

        $arms = [];
        for ($idx = $start + 1; $idx < count($lines); $idx++) {
            $line = $lines[$idx];
            if ($line === '}') {
                if ($arms === []) {
                    $errors[] = self::DISPATCH . ': `construct_entity` has no `b"FQCN" =>` arm';
                }

                return $arms;
            }
            if (!preg_match('/^\s*b"((?:[^"\\\\]|\\\\.)*)"\s*=>/', $line, $m)) {
                continue;
            }
            if (!preg_match('/^(?:\w|\\\\\\\\)+$/', $m[1])) {
                $errors[] = self::DISPATCH . ':' . ($idx + 1) . ": cannot read the arm pattern `b\"{$m[1]}\"`";
                continue;
            }
            $arms[] = str_replace('\\\\', '\\', $m[1]);
        }

        $errors[] = self::DISPATCH . ': `construct_entity` has no closing `}` at column 0';

        return [];
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
