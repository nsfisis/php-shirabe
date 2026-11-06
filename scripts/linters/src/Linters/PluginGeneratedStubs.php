<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * The committed proxy stub and guard classes have to match what
 * `scripts/plugin-stub-generator/generate-stubs` emits from the current Composer checkout and
 * classifier report.
 */
final class PluginGeneratedStubs implements Linter
{
    public function name(): string
    {
        return 'plugin_generated_stubs';
    }

    public function failureIntro(): string
    {
        return 'scripts/plugin-stub-generator/generate-stubs --check failed:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $tool = "{$rootDir}/scripts/plugin-stub-generator/generate-stubs";
        exec(escapeshellarg(PHP_BINARY) . ' ' . escapeshellarg($tool) . ' --check 2>&1', $output, $status);

        return $status === 0 ? [] : $output;
    }
}
