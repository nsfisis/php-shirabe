<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

/**
 * A ported exception travels as an `AnyThrowable`, never as itself, so `downcast_ref::<X>()`
 * against an exception type compiles and always answers `None`. It would also answer only for the
 * exact class, where PHP's `catch` answers for every subclass too.
 */
final class NoExceptionDowncast implements Linter
{
    private const DOWNCAST_RE = '/\bdowncast(?:_ref|_mut)?::<\s*(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*([A-Za-z_][A-Za-z0-9_]*)\s*>/';

    private const EXCEPTION_DEFINITION_RE = '/\b(?:impl_php_exception|define_php_exception)!\(\s*(?:@accessors\s+)?\$?([A-Za-z_][A-Za-z0-9_]*)/';

    public function name(): string
    {
        return 'no_exception_downcast';
    }

    public function failureIntro(): string
    {
        return "Found `downcast` against a ported PHP exception type.\n"
            . 'An exception is carried by an `AnyThrowable`, so this never matches; '
            . 'use `Catch::catch` / `Catch::catch_mut`, which match subclasses too:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $exceptionTypes = $this->exceptionTypes($rootDir);
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (file($path) as $idx => $line) {
                if (!preg_match(self::DOWNCAST_RE, $line, $m)) {
                    continue;
                }
                if (!isset($exceptionTypes[$m[1]])) {
                    continue;
                }

                $errors[] = "{$relative}:" . ($idx + 1) . ": downcast to `{$m[1]}`";
            }
        }

        return $errors;
    }

    /** @return array<string, true> the type name of every ported exception */
    private function exceptionTypes(string $rootDir): array
    {
        $types = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            preg_match_all(self::EXCEPTION_DEFINITION_RE, file_get_contents($path), $matches);
            foreach ($matches[1] as $name) {
                if ($name !== 'ty') {
                    $types[$name] = true;
                }
            }
        }

        return $types;
    }
}
