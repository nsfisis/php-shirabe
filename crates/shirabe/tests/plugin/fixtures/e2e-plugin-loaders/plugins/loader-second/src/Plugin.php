<?php

namespace ShirabeTest\LoaderSecond;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use ShirabeTest\LoaderFirst\Plugin as FirstPlugin;

/**
 * On activation, while the latest class loader registered under the vendor directory is its own,
 * makes the first plugin load a class it has not loaded yet.
 */
class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        FirstPlugin::trace('activate second');
        FirstPlugin::trace('second lazy-exists=' . json_encode(class_exists('ShirabeTest\LoaderFirst\Lazy')));
        FirstPlugin::loadLazyClass();
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }
}
