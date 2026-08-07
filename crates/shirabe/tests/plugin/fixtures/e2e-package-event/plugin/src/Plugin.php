<?php

namespace ShirabeTest\PackageEvent;

use Composer\Composer;
use Composer\DependencyResolver\Operation\InstallOperation;
use Composer\DependencyResolver\Operation\OperationInterface;
use Composer\DependencyResolver\Operation\UninstallOperation;
use Composer\DependencyResolver\Operation\UpdateOperation;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Installer\PackageEvent;
use Composer\Installer\PackageEvents;
use Composer\Plugin\PluginInterface;

/**
 * Appends one line per PackageEvent to package-event-trace.txt, so both the order the events
 * arrive in and everything the event exposes are comparable between implementations.
 *
 * The local repository is observed into a second file, because its size at the moment an event
 * fires reports how far the batch's operations have run rather than anything the event carries.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    /** @var IOInterface */
    private $io;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->io = $io;
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
            PackageEvents::PRE_PACKAGE_INSTALL => 'onPackageEvent',
            PackageEvents::POST_PACKAGE_INSTALL => 'onPackageEvent',
            PackageEvents::PRE_PACKAGE_UPDATE => 'onPackageEvent',
            PackageEvents::POST_PACKAGE_UPDATE => 'onPackageEvent',
            PackageEvents::PRE_PACKAGE_UNINSTALL => 'onPackageEvent',
            PackageEvents::POST_PACKAGE_UNINSTALL => 'onPackageEvent',
        ];
    }

    public function onPackageEvent(PackageEvent $event): void
    {
        $operation = $event->getOperation();
        $line = implode(' ', [
            $event->getName(),
            'devMode=' . ($event->isDevMode() ? '1' : '0'),
            'class=' . get_class($operation),
            'type=' . $operation->getOperationType(),
            'packages=' . $this->packages($operation),
            'operations=' . count($event->getOperations()),
            'root=' . $event->getComposer()->getPackage()->getName(),
            'show=' . $operation->show(false),
        ]);
        $this->io->write('package-event: ' . $line);
        file_put_contents('package-event-trace.txt', $line . "\n", FILE_APPEND);
        file_put_contents(
            'package-event-repo-trace.txt',
            $event->getName() . ' localRepo=' . count($event->getLocalRepo()->getPackages()) . "\n",
            FILE_APPEND
        );
    }

    private function packages(OperationInterface $operation): string
    {
        if ($operation instanceof UpdateOperation) {
            return $operation->getInitialPackage()->getPrettyName()
                . '->' . $operation->getTargetPackage()->getPrettyName();
        }
        if ($operation instanceof InstallOperation || $operation instanceof UninstallOperation) {
            return $operation->getPackage()->getPrettyName();
        }

        return 'n/a';
    }
}
