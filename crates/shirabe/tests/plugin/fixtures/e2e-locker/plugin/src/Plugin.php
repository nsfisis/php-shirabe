<?php

namespace ShirabeTest\Locker;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\Installer\PackageEvent;
use Composer\Installer\PackageEvents;
use Composer\DependencyResolver\Operation\InstallOperation;
use Composer\IO\IOInterface;
use Composer\Package\Locker;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

/**
 * Reads the Locker that Composer::getLocker() hands out and appends what every call reports to
 * locker-trace.txt, so the surface can be compared line by line between implementations: the
 * lock state while the plugin itself is being installed (what wikimedia/composer-merge-plugin
 * records), the lock contents after the lock file is written and when it is replayed, and an
 * in-memory setLockData() round trip.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    /** @var Composer */
    private $composer;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->composer = $composer;
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
            PackageEvents::POST_PACKAGE_INSTALL => 'onPostPackageInstall',
            ScriptEvents::POST_INSTALL_CMD => 'onPostCommand',
            ScriptEvents::POST_UPDATE_CMD => 'onPostCommand',
        ];
    }

    public function onPostPackageInstall(PackageEvent $event): void
    {
        $operation = $event->getOperation();
        if ($operation instanceof InstallOperation && $operation->getPackage()->getName() === 'shirabe-test/locker-probe') {
            $this->write(['installed isLocked=' . json_encode($event->getComposer()->getLocker()->isLocked())]);
        }
    }

    public function onPostCommand(Event $event): void
    {
        $composer = $this->composer;
        $locker = $composer->getLocker();

        $lines = ['event=' . $event->getName()];
        $lines[] = 'class=' . get_class($locker) . ' same=' . json_encode($locker === $composer->getLocker());
        $lines[] = 'isLocked=' . json_encode($locker->isLocked()) . ' isFresh=' . json_encode($locker->isFresh());

        foreach ([false, true] as $withDev) {
            $repository = $locker->getLockedRepository($withDev);
            $names = [];
            foreach ($repository->getPackages() as $package) {
                $names[] = get_class($package) . ':' . $package->getName() . '@' . $package->getPrettyVersion();
            }
            $lines[] = 'lockedRepository dev=' . json_encode($withDev) . ' class=' . get_class($repository) . ' ' . json_encode($names);

            $requirements = [];
            foreach ($locker->getPlatformRequirements($withDev) as $name => $link) {
                $requirements[] = $name . '=' . get_class($link) . ':' . $link->getPrettyConstraint();
            }
            $lines[] = 'platformRequirements dev=' . json_encode($withDev) . ' ' . json_encode($requirements);
        }

        $lines[] = 'devPackageNames ' . json_encode($locker->getDevPackageNames());
        $lines[] = 'minimumStability ' . json_encode($locker->getMinimumStability());
        $lines[] = 'stabilityFlags ' . json_encode($locker->getStabilityFlags());
        $lines[] = 'preferStable ' . json_encode($locker->getPreferStable());
        $lines[] = 'preferLowest ' . json_encode($locker->getPreferLowest());
        $lines[] = 'platformOverrides ' . json_encode($locker->getPlatformOverrides());
        $lines[] = 'aliases ' . json_encode($locker->getAliases());
        $lines[] = 'pluginApi ' . json_encode($locker->getPluginApi());

        $lockData = $locker->getLockData();
        $lines[] = 'lockData keys ' . json_encode(array_keys($lockData));
        $lines[] = 'lockData contentHash matches=' . json_encode(
            $lockData['content-hash'] === Locker::getContentHash((string) file_get_contents('composer.json'))
        );
        $lines[] = 'missingRequirementInfo ' . json_encode($locker->getMissingRequirementInfo($composer->getPackage(), true));

        $generator = $composer->getAutoloadGenerator();
        $generator->setDryRun(true);
        $generator->setRunScripts(false);
        $classMap = $generator->dump(
            $composer->getConfig(),
            $composer->getRepositoryManager()->getLocalRepository(),
            $composer->getPackage(),
            $composer->getInstallationManager(),
            'composer',
            false,
            'Probe',
            $locker
        );
        $generator->setRunScripts(true);
        $generator->setDryRun(false);
        $lines[] = 'dump withLocker count=' . count($classMap);

        if ($event->getName() === ScriptEvents::POST_INSTALL_CMD) {
            $packages = $locker->getLockedRepository()->getPackages();
            $devPackages = [];
            foreach ($locker->getLockedRepository(true)->getPackages() as $package) {
                if (in_array($package->getName(), $locker->getDevPackageNames(), true)) {
                    $devPackages[] = $package;
                }
            }
            $changed = $locker->setLockData(
                $packages,
                $devPackages,
                ['php' => '>=7.0'],
                ['ext-json' => '*'],
                $locker->getAliases(),
                'dev',
                ['acme/lib-b' => 20],
                false,
                true,
                ['php' => '8.1.0'],
                false
            );
            $lines[] = 'setLockData changed=' . json_encode($changed)
                . ' minimumStability=' . json_encode($locker->getMinimumStability())
                . ' preferStable=' . json_encode($locker->getPreferStable())
                . ' preferLowest=' . json_encode($locker->getPreferLowest())
                . ' isFresh=' . json_encode($locker->isFresh());
        }

        $this->write($lines);
    }

    private function write(array $lines): void
    {
        file_put_contents('locker-trace.txt', implode("\n", $lines) . "\n", FILE_APPEND);
    }
}
