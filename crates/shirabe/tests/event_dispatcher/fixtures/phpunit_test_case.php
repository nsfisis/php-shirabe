<?php declare(strict_types=1);

namespace PHPUnit\Framework;

/**
 * Stands in for the base class of `Composer\Test\TestCase`, which the Composer PHP runtime
 * bundle does not carry. Declaring it is all the worker needs to require the upstream
 * `EventDispatcherTest` from the Composer checkout and call its listener methods.
 *
 * The two assertions are the only PHPUnit members those listeners use, and they report failure
 * the way PHPUnit does, by throwing.
 */
class TestCase
{
    public static function assertStringContainsString(string $needle, string $haystack, string $message = ''): void
    {
        if (!str_contains($haystack, $needle)) {
            throw new \RuntimeException($message !== '' ? $message : sprintf(
                'Failed asserting that %s contains %s.',
                var_export($haystack, true),
                var_export($needle, true)
            ));
        }
    }

    public static function assertStringNotContainsString(string $needle, string $haystack, string $message = ''): void
    {
        if (str_contains($haystack, $needle)) {
            throw new \RuntimeException($message !== '' ? $message : sprintf(
                'Failed asserting that %s does not contain %s.',
                var_export($haystack, true),
                var_export($needle, true)
            ));
        }
    }
}
