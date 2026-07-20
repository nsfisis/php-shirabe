<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class NoDecorativeSectionComment implements Linter
{
    // 4+ consecutive ASCII `-`/`=`, or 4+ consecutive Unicode box-drawing characters (U+2500-U+257F).
    private const DECORATIVE_RUN_RE = '/[-=]{4,}|[\x{2500}-\x{257F}]{4,}/u';

    public function name(): string
    {
        return 'no_decorative_section_comment';
    }

    public function failureIntro(): string
    {
        return "Found decorative section comments (4+ consecutive `=`, `-`, or Unicode box-drawing characters).\n"
            . 'These section dividers are unnecessarily noisy — remove them:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findDecorativeComments($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findDecorativeComments(string $path, string $relative): array
    {
        $errors = [];

        foreach (file($path) as $idx => $raw) {
            $stripped = ltrim($raw);
            if (!str_starts_with($stripped, '//')) {
                continue;
            }
            if (str_starts_with($stripped, '///') || str_starts_with($stripped, '//!')) {
                continue;
            }
            if (!preg_match(self::DECORATIVE_RUN_RE, $stripped)) {
                continue;
            }

            $errors[] = "{$relative}:" . ($idx + 1) . ': decorative section comment';
        }

        return $errors;
    }
}
