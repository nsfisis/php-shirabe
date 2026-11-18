<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * The committed classifier report has to match what `scripts/plugin-class-classifier/classify`
 * computes from the current Composer checkout and the classifier inputs, with no violations.
 */
final class PluginClassification implements Linter
{
    public function name(): string
    {
        return 'plugin_classification';
    }

    public function failureIntro(): string
    {
        return 'scripts/plugin-class-classifier/classify --check failed:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $tool = "{$rootDir}/scripts/plugin-class-classifier/classify";
        exec(escapeshellarg(PHP_BINARY) . ' ' . escapeshellarg($tool) . ' --check 2>&1', $output, $status);

        return $status === 0 ? [] : $output;
    }
}
