<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

final class ContiguousUseBlock implements Linter
{
    private const USE_START_RE = '/\A(?:pub(?:\([^)]*\))?\s+)?use\b/';

    public function name(): string
    {
        return 'contiguous_use_block';
    }

    public function failureIntro(): string
    {
        return "Found blank lines splitting the leading `use` block into sections.\n"
            . 'All `use` statements at the top of the file must be contiguous (no blank lines between them):';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            array_push($errors, ...$this->findSplitUseBlock($path, $relative));
        }

        return $errors;
    }

    /** @return list<string> */
    private function findSplitUseBlock(string $path, string $relative): array
    {
        $lines = file($path);
        $errors = [];
        $count = count($lines);

        $i = $this->skipPreamble($lines);
        if ($i === null) {
            return [];
        }

        while (true) {
            $i = $this->consumeUseStatement($lines, $i);
            if ($i >= $count) {
                break;
            }

            $blanks = [];
            $j = $i;
            while ($j < $count) {
                $stripped = trim($lines[$j]);
                if ($stripped === '') {
                    $blanks[] = $j;
                    $j++;
                } elseif (str_starts_with($stripped, '//') || str_starts_with($stripped, '#[')) {
                    $j++;
                } else {
                    break;
                }
            }

            if ($j < $count && preg_match(self::USE_START_RE, trim($lines[$j]))) {
                foreach ($blanks as $bi) {
                    $errors[] = "{$relative}:" . ($bi + 1) . ': blank line splits the leading `use` block';
                }
                $i = $j;
            } else {
                break;
            }
        }

        return $errors;
    }

    /** @param list<string> $lines */
    private function skipPreamble(array $lines): ?int
    {
        foreach ($lines as $idx => $raw) {
            $stripped = trim($raw);
            if (preg_match(self::USE_START_RE, $stripped)) {
                return $idx;
            }
            if ($stripped === '' || str_starts_with($stripped, '//') || str_starts_with($stripped, '#![') || str_starts_with($stripped, '#[')) {
                continue;
            }

            return null;
        }

        return null;
    }

    /** @param list<string> $lines */
    private function consumeUseStatement(array $lines, int $startIdx): int
    {
        $braceDepth = 0;
        $i = $startIdx;
        $count = count($lines);

        while ($i < $count) {
            $line = $lines[$i];
            $braceDepth += substr_count($line, '{') - substr_count($line, '}');
            $done = $braceDepth <= 0 && str_ends_with(rtrim($line), ';');
            $i++;
            if ($done) {
                return $i;
            }
        }

        return $i;
    }
}
