<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoDirectEnvAccess implements Linter
{
    private const BANNED_ENV_FUNCTIONS = ['var', 'var_os', 'vars', 'vars_os', 'set_var', 'remove_var'];

    public function name(): string
    {
        return 'no_direct_env_access';
    }

    public function failureIntro(): string
    {
        return "Found direct access to the process environment.\n"
            . "PHP reads and writes environment variables through three storages that are not kept in\n"
            . "sync with each other, and each has its own counterpart in `shirabe_php_shim`:\n"
            . "`getenv`/`getenv_all`/`putenv`/`putenv_clear`, `PHP_ENV`, and `PHP_SERVER`.\n"
            . 'Port whichever one the PHP source uses; see `docs/dev/env-vars-porting.md`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findEnvAccesses($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findEnvAccesses(string $path, string $relative): array
    {
        $errors = [];
        $names = implode('|', self::BANNED_ENV_FUNCTIONS);

        foreach (file($path) as $idx => $raw) {
            $code = explode('//', $raw, 2)[0];

            if (preg_match_all("/\bstd::env::({$names})\b/", $code, $m)) {
                foreach ($m[1] as $name) {
                    $errors[] = "{$relative}:" . ($idx + 1) . ": use of `std::env::{$name}`";
                }
            }

            if (preg_match_all('/\bstd::env::\{([^}]*)\}/', $code, $m)) {
                foreach ($m[1] as $group) {
                    foreach (explode(',', $group) as $entry) {
                        $name = preg_split('/\s+as\s+/', trim($entry))[0];
                        if (!in_array($name, self::BANNED_ENV_FUNCTIONS, true)) {
                            continue;
                        }
                        $errors[] = "{$relative}:" . ($idx + 1) . ": import of `std::env::{$name}`";
                    }
                }
            }
        }

        return array_values(array_unique($errors));
    }
}
