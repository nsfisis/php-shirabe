<?php

namespace ShirabeTest\GlobalHelper;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

/**
 * Leaves its `ScriptHandler` class unloaded, so the first lookup for it comes from the project's script.
 */
class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        Helper::trace('activate handler-loaded=' . json_encode(class_exists(ScriptHandler::class, false)));
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }
}
