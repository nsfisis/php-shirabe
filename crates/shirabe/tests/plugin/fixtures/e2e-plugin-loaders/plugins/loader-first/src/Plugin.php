<?php

namespace ShirabeTest\LoaderFirst;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

/**
 * Leaves its `Lazy` class unloaded until the second plugin, activated after its own class loader
 * has been registered under the same vendor directory, asks for it. Each step appends a line to
 * plugin-loaders-trace.txt.
 */
class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        self::trace('activate first lazy-loaded=' . json_encode(class_exists(Lazy::class, false)));
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }

    public static function loadLazyClass(): void
    {
        self::trace('first lazy-loaded=' . json_encode(class_exists(Lazy::class, false)));
        self::trace('first ' . (new Lazy())->describe());
    }

    public static function trace(string $line): void
    {
        file_put_contents('plugin-loaders-trace.txt', $line . "\n", FILE_APPEND);
    }
}
