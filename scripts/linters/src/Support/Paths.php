<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

final class Paths
{
    public static function relativeTo(string $rootDir, string $path): string
    {
        $root = rtrim($rootDir, '/') . '/';

        return str_starts_with($path, $root) ? substr($path, strlen($root)) : $path;
    }
}
