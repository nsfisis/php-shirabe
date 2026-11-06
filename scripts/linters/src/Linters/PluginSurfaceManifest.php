<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * `crates/shirabe/src/plugin/rpc/surface.tsv` has to match what
 * `scripts/plugin-stub-generator/surface-manifest` derives from the committed stubs and runtime
 * classes and from the match arms of the Rust dispatchers.
 */
final class PluginSurfaceManifest implements Linter
{
    public function name(): string
    {
        return 'plugin_surface_manifest';
    }

    public function failureIntro(): string
    {
        return 'scripts/plugin-stub-generator/surface-manifest --check failed:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $tool = "{$rootDir}/scripts/plugin-stub-generator/surface-manifest";
        exec(escapeshellarg(PHP_BINARY) . ' ' . escapeshellarg($tool) . ' --check 2>&1', $output, $status);

        return $status === 0 ? [] : $output;
    }
}
