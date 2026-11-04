<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

// The `//! ref:` header naming the upstream file a Rust file ports, with the SHA-1 of that file's
// content. The definition is documented in docs/dev/porting-rules.md ("File and directory layout").
final class RefHeader
{
    // Any `//! ref:` line, so that one without a hash is reported instead of being skipped.
    public const CANDIDATE_RE = '/^\/\/! ref:/';

    // `//! ref: composer/src/Composer/Json/JsonFile.php sha1:<40 hex>`
    public const HEADER_RE = '/^\/\/! ref: (\S+) sha1:([0-9a-f]{40})$/';

    // A header with or without the hash, as `update` accepts it.
    public const UPDATABLE_RE = '/^\/\/! ref: (\S+)( sha1:[0-9a-f]{40})?$/';

    public const SOURCE_PREFIX = 'composer/';

    public static function format(string $file, string $hash): string
    {
        return "//! ref: {$file} sha1:{$hash}";
    }

    /**
     * Returns the SHA-1 of the upstream file `$file`, relative to `$rootDir`.
     *
     * @throws \RuntimeException when the file is not under composer/ or cannot be read
     */
    public static function hash(string $rootDir, string $file): string
    {
        if (!str_starts_with($file, self::SOURCE_PREFIX)) {
            throw new \RuntimeException("`{$file}` is not under `" . self::SOURCE_PREFIX . '`');
        }
        $path = "{$rootDir}/{$file}";
        if (!is_file($path)) {
            throw new \RuntimeException("`{$file}` does not exist");
        }
        $hash = sha1_file($path);
        if ($hash === false) {
            throw new \RuntimeException("`{$file}` cannot be read");
        }

        return $hash;
    }
}
