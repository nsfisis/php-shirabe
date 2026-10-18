<?php

namespace ShirabeTest\InstallerEvent;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Installer\InstallerEvent;
use Composer\Installer\InstallerEvents;
use Composer\Plugin\PluginInterface;

/**
 * Appends one line per InstallerEvent listener call to installer-event-trace.txt, so both the
 * order the listeners run in and everything the event exposes are comparable between
 * implementations.
 *
 * The second listener stops propagation, so the third one must never run.
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
            InstallerEvents::PRE_OPERATIONS_EXEC => [
                ['onRecord', 10],
                ['onStop', 5],
                ['onAfterStop', 0],
            ],
        ];
    }

    public function onRecord(InstallerEvent $event): void
    {
        $transaction = $event->getTransaction();
        $operations = $transaction->getOperations();
        $shown = array_map(static function ($operation): string {
            return get_class($operation) . ':' . $operation->show(false);
        }, $operations);
        $line = implode(' ', [
            $event->getName(),
            'class=' . get_class($event),
            'devMode=' . ($event->isDevMode() ? '1' : '0'),
            'executing=' . ($event->isExecutingOperations() ? '1' : '0'),
            'transaction=' . get_class($transaction),
            'operations=' . count($operations),
            'identical=' . ($operations === $transaction->getOperations() ? '1' : '0'),
            'root=' . $event->getComposer()->getPackage()->getName(),
            'io=' . ($event->getIO() instanceof IOInterface ? '1' : '0'),
            'shown=' . implode(',', $shown),
        ]);
        $this->trace($line);
    }

    public function onStop(InstallerEvent $event): void
    {
        $this->trace('stop ' . ($event->isPropagationStopped() ? '1' : '0'));
        $event->stopPropagation();
        $this->trace('stopped ' . ($event->isPropagationStopped() ? '1' : '0'));
    }

    public function onAfterStop(InstallerEvent $event): void
    {
        $this->trace('after-stop');
    }

    private function trace(string $line): void
    {
        file_put_contents('installer-event-trace.txt', $line . "\n", FILE_APPEND);
    }
}
