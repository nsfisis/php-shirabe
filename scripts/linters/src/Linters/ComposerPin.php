<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;

/**
 * The Composer sources are pinned twice: as the `composer` submodule, which a `cargo` build reads,
 * and as the `composer` flake input, which a `nix build` reads in its place because a flake source
 * carries no submodule. Both have to name the same commit, or the two builds embed different
 * Composer runtimes.
 */
final class ComposerPin implements Linter
{
    public function name(): string
    {
        return 'composer_pin';
    }

    public function failureIntro(): string
    {
        return "The `composer` submodule and the `composer` flake input name different commits.\n"
            . 'Point the input at the submodule commit and run `nix flake lock`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $submoduleRev = $this->submoduleRev($rootDir);
        $inputRev = $this->inputRev($rootDir);

        if ($submoduleRev === $inputRev) {
            return [];
        }

        return [
            "submodule:   {$submoduleRev}",
            "flake input: {$inputRev}",
        ];
    }

    private function submoduleRev(string $rootDir): string
    {
        $command = sprintf('git -C %s ls-tree HEAD composer', escapeshellarg($rootDir));
        exec($command, $output, $status);
        if ($status !== 0 || $output === []) {
            return '(unreadable: git ls-tree HEAD composer failed)';
        }

        // `<mode> commit <rev>\tcomposer`
        $fields = preg_split('/\s+/', $output[0]);

        return $fields[2] ?? '(unreadable: unexpected git ls-tree output)';
    }

    private function inputRev(string $rootDir): string
    {
        $lock = json_decode((string) file_get_contents("{$rootDir}/flake.lock"), true);

        return $lock['nodes']['composer']['locked']['rev'] ?? '(missing from flake.lock)';
    }
}
