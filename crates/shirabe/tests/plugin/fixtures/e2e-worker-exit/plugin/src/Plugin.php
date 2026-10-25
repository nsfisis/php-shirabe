<?php

namespace ShirabeTest\WorkerExit;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\ScriptEvents;

/**
 * Ends the PHP process from a listener, in the way the SHIRABE_E2E_WORKER_EXIT environment
 * variable names: `exit` calls exit(3), `fatal` runs out of memory, and `kill` sends the process
 * SIGKILL. Under Composer that is the process running Composer itself.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }

    public static function getSubscribedEvents()
    {
        return [
            ScriptEvents::POST_INSTALL_CMD => 'onPostCommand',
            ScriptEvents::POST_UPDATE_CMD => 'onPostCommand',
        ];
    }

    public function onPostCommand(): void
    {
        switch (getenv('SHIRABE_E2E_WORKER_EXIT')) {
            case 'exit':
                exit(3);
            case 'fatal':
                ini_set('memory_limit', (string) (memory_get_usage() + 32 * 1024 * 1024));
                $chunks = [];
                while (true) {
                    $chunks[] = str_repeat('x', 1024 * 1024);
                }
                // no break
            case 'kill':
                posix_kill(getmypid(), 9);
        }
    }
}
