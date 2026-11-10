<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

/**
 * In the `shirabe` crate, a Rust handle descriptor (`RustObjHandle { ... }`) is built only by
 * `entity_value`, which registers the entity and takes the class from its `Entity::class`, so an
 * rhandle can never cross the boundary under a class its entity does not cross as.
 */
final class RustHandleDescriptorConstruction implements Linter
{
    public function name(): string
    {
        return 'rust_handle_descriptor_construction';
    }

    public function failureIntro(): string
    {
        return "Found a `RustObjHandle` built outside `entity_value`.\n"
            . 'Hand the entity to `crates/shirabe/src/php_worker/rpc/entity.rs` `entity_value` instead:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];
        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (!str_starts_with($relative, 'crates/shirabe/') || in_array($relative, $excludes, true)) {
                continue;
            }
            foreach (file($path) as $idx => $raw) {
                if (preg_match('/\bRustObjHandle\s*\{/', $raw)) {
                    $errors[] = "{$relative}:" . ($idx + 1) . ': ' . trim($raw);
                }
            }
        }

        return $errors;
    }
}
