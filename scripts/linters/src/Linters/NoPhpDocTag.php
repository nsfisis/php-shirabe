<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoPhpDocTag implements Linter
{
    // `/// @param ...`, `//! @internal`, or a doc line consisting of `{@inheritDoc}` alone.
    private const TAG_RE = '/^\/\/[\/!]\s*(?:@([A-Za-z][A-Za-z-]*)|\{@inheritdoc\}\s*$)/i';

    // Original authors' TODOs are kept verbatim.
    private const ALLOWED_TAGS = ['TODO'];

    public function name(): string
    {
        return 'no_phpdoc_tag';
    }

    public function failureIntro(): string
    {
        return "Found PHPDoc tags in doc comments.\n"
            . 'Types and visibility are expressed in Rust, and the PHP source documents the rest — remove them:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findPhpDocTags($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findPhpDocTags(string $path, string $relative): array
    {
        $errors = [];

        foreach (file($path) as $idx => $raw) {
            if (!preg_match(self::TAG_RE, ltrim($raw), $m)) {
                continue;
            }
            if (isset($m[1]) && $m[1] !== '' && in_array($m[1], self::ALLOWED_TAGS, true)) {
                continue;
            }

            $errors[] = "{$relative}:" . ($idx + 1) . ': PHPDoc tag';
        }

        return $errors;
    }
}
