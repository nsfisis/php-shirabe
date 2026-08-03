<?php

namespace Subscriber;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

class Plugin implements PluginInterface, EventSubscriberInterface
{
    public $version = 'subscriber-v1';

    /** @var IOInterface */
    private $io;

    public function activate(Composer $composer, IOInterface $io)
    {
        $this->io = $io;
        $io->write('activate subscriber-v1');
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
    }

    public static function getSubscribedEvents()
    {
        return [
            'post-install-cmd' => 'onPostInstall',
            'shirabe-priority-event' => [['early', 10], ['late', -10]],
            'shirabe-false-event' => ['returnsFalse', 0],
        ];
    }

    public function onPostInstall($event)
    {
        $this->io->write('subscriber saw ' . $event->getName());
    }

    public function early($event)
    {
        $this->io->write('early listener');
    }

    public function late($event)
    {
        $this->io->write('late listener');
    }

    public function returnsFalse($event)
    {
        $this->io->write('failing listener');

        return false;
    }
}
