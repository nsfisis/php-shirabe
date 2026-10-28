<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

// PHP strings are byte strings. Every call that decodes bytes, paths or OS strings as UTF-8 either
// corrupts bytes that are not valid UTF-8 (`*_lossy`), fails where PHP does not (`to_str()`,
// `from_utf8()`, `into_string()`), or both. The calls that predate this rule are recorded per file
// in a baseline, which may only shrink.
final class NoUtf8Gate implements Linter
{
    public const BASELINE = 'scripts/linters/baselines/no_utf8_gate.txt';

    public const EXCLUDES = [
        // Defines `legacy_lossy()`, the one conversion the rule points at.
        'crates/shirabe-php-shim/src/bytes.rs',
    ];

    private const PATTERNS = [
        '/\bto_string_lossy\b/',
        '/\bfrom_utf8_lossy(?:_owned)?\b/',
        '/\bto_str_lossy(?:_into)?\b/',
        '/\binto_string_lossy\b/',
        '/\bfrom_utf8(?:_unchecked)?\b/',
        '/\.to_str\(\)/',
        '/\.into_string\(\)/',
    ];

    public function name(): string
    {
        return 'no_utf8_gate';
    }

    public function failureIntro(): string
    {
        return "Found UTF-8 conversions of PHP strings.\n"
            . "PHP strings are byte strings: keep the bytes (`BString`, `&[u8]`, `Path`). Decode them only\n"
            . "inside a shim of a PHP function that itself requires UTF-8, reproducing how it fails.\n"
            . "At the boundary with code that still holds `String`, use `legacy_lossy()`.\n"
            . 'If the change removes conversions, run `scripts/linters/update-baseline`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $baseline = self::readBaseline($rootDir);
        $errors = [];

        foreach (self::count($rootDir, $excludes) as $relative => $lines) {
            $allowed = $baseline[$relative] ?? 0;
            unset($baseline[$relative]);
            if (count($lines) > $allowed) {
                $errors[] = "{$relative}: " . count($lines) . " conversions, baseline {$allowed}";
                foreach ($lines as $line) {
                    $errors[] = "    {$line}";
                }
            } elseif (count($lines) < $allowed) {
                $errors[] = "{$relative}: " . count($lines) . " conversions, baseline {$allowed} (update the baseline)";
            }
        }

        foreach ($baseline as $relative => $allowed) {
            $errors[] = "{$relative}: 0 conversions, baseline {$allowed} (update the baseline)";
        }

        return $errors;
    }

    /**
     * @param list<string> $excludes
     * @return array<string, list<string>> violation lines keyed by root-relative path
     */
    public static function count(string $rootDir, array $excludes): array
    {
        $found = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach (file($path) as $idx => $raw) {
                $code = explode('//', $raw, 2)[0];
                foreach (self::PATTERNS as $pattern) {
                    $n = preg_match_all($pattern, $code);
                    for ($i = 0; $i < $n; $i++) {
                        $found[$relative][] = ($idx + 1) . ': ' . trim($raw);
                    }
                }
            }
        }

        return $found;
    }

    /** @param array<string, list<string>> $counts */
    public static function writeBaseline(string $rootDir, array $counts): void
    {
        ksort($counts);
        $out = '';
        foreach ($counts as $relative => $lines) {
            $out .= count($lines) . " {$relative}\n";
        }
        file_put_contents("{$rootDir}/" . self::BASELINE, $out);
    }

    /** @return array<string, int> */
    private static function readBaseline(string $rootDir): array
    {
        $baseline = [];
        foreach (file("{$rootDir}/" . self::BASELINE, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [] as $line) {
            [$n, $relative] = explode(' ', $line, 2);
            $baseline[$relative] = (int) $n;
        }

        return $baseline;
    }
}
