<?php

declare(strict_types=1);

namespace Shirabe\Lint\Support;

// A `ref:` line naming an upstream file, with the SHA-1 of that file's content. It is either the
// `//! ref:` header of a Rust file that ports the upstream file, a `// ref:` comment next to an
// `include_str!` / `include_bytes!` that embeds it, or a `// ref:` comment in the PHP code the
// worker runs that ports or depends on it. The definition is documented in
// docs/dev/porting-rules.md ("File and directory layout").
final class RefHeader
{
    // Any `ref:` line, so that one without a hash is reported instead of being skipped. A `// ref:`
    // comment counts only when it names a file under composer/: the tests also write `// ref:`
    // followed by the upstream test method a helper stands for. A doc comment naming a file under
    // composer/ is matched only to be reported, as it is never checked otherwise.
    public const CANDIDATE_RE = '/^(?:\/\/! ref:|\s*\/\/ ref: composer\/|\s*\/\/\/\s*ref:\s*composer\/)/';

    // `//! ref: composer/src/Composer/Json/JsonFile.php sha1:<40 hex>`, or the same with an
    // indented `//`. Groups: the leading comment marker, the file, the hash.
    public const HEADER_RE = '/^(\/\/!|\s*\/\/) ref: (\S+) sha1:([0-9a-f]{40})$/';

    // A `ref:` line with or without the hash, as `update` accepts it.
    public const UPDATABLE_RE = '/^(\/\/!|\s*\/\/) ref: (\S+)(?: sha1:[0-9a-f]{40})?$/';

    public const SOURCE_PREFIX = 'composer/';

    public static function format(string $marker, string $file, string $hash): string
    {
        return "{$marker} ref: {$file} sha1:{$hash}";
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
