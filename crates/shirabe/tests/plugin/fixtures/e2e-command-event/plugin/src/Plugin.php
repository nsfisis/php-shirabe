<?php

namespace ShirabeTest\CommandEvent;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\CommandEvent;
use Composer\Plugin\PluginEvents;
use Composer\Plugin\PluginInterface;
use Composer\Plugin\PreCommandRunEvent;

/**
 * Appends one line per command event to command-event-trace.txt, and what the console input and
 * output the events carry report to command-event-console-trace.txt, so both are comparable
 * between implementations.
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
            PluginEvents::PRE_COMMAND_RUN => 'onPreCommandRun',
            PluginEvents::COMMAND => 'onCommand',
        ];
    }

    public function onPreCommandRun(PreCommandRunEvent $event): void
    {
        $this->trace('command-event-trace.txt', $event->getName() . ' class=' . get_class($event) . ' command=' . $event->getCommand());
        $this->trace('command-event-console-trace.txt', $event->getName() . ' ' . $this->console(static function () use ($event): string {
            return 'input=' . $event->getInput()->getFirstArgument();
        }));
    }

    public function onCommand(CommandEvent $event): void
    {
        $this->trace('command-event-trace.txt', $event->getName() . ' class=' . get_class($event) . ' commandName=' . $event->getCommandName());
        $this->trace('command-event-console-trace.txt', $event->getName() . ' ' . $this->console(static function () use ($event): string {
            return 'input=' . $event->getInput()->getFirstArgument()
                . ' verbosity=' . $event->getOutput()->getVerbosity();
        }));
    }

    private function console(callable $read): string
    {
        try {
            return $read();
        } catch (\Throwable $e) {
            return 'threw ' . get_class($e);
        }
    }

    private function trace(string $file, string $line): void
    {
        file_put_contents($file, $line . "\n", FILE_APPEND);
    }
}
