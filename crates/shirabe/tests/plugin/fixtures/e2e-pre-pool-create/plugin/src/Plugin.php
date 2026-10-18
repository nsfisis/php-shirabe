<?php

namespace ShirabeTest\PrePoolCreate;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Package\BasePackage;
use Composer\Plugin\PluginEvents;
use Composer\Plugin\PluginInterface;
use Composer\Plugin\PrePoolCreateEvent;

/**
 * Appends one line per PrePoolCreateEvent to pre-pool-create-trace.txt, so everything the event
 * exposes is comparable between implementations, and drops shirabe-test/pool-lib 2.0.0 from the
 * pool, so the solver only picks 1.0.0 when the filtered list is the one it reads.
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
            PluginEvents::PRE_POOL_CREATE => 'onPrePoolCreate',
        ];
    }

    public function onPrePoolCreate(PrePoolCreateEvent $event): void
    {
        $packages = $event->getPackages();
        $names = $this->names($packages);
        $kept = array_values(array_filter($packages, static function (BasePackage $package): bool {
            return !($package->getName() === 'shirabe-test/pool-lib' && $package->getVersion() === '2.0.0.0');
        }));
        $event->setPackages($kept);
        $event->setUnacceptableFixedPackages($event->getUnacceptableFixedPackages());

        $line = implode(' ', [
            $event->getName(),
            'class=' . get_class($event),
            'packages=' . implode(',', $names),
            'kept=' . implode(',', $this->names($event->getPackages())),
            'unacceptable=' . count($event->getUnacceptableFixedPackages()),
            'stabilities=' . json_encode($event->getAcceptableStabilities()),
            'flags=' . json_encode($event->getStabilityFlags()),
            'aliases=' . json_encode($event->getRootAliases()),
            'references=' . json_encode($event->getRootReferences()),
        ]);
        file_put_contents('pre-pool-create-trace.txt', $line . "\n", FILE_APPEND);
    }

    /**
     * The sorted names of the vendor packages in the list: the platform packages depend on the
     * PHP build running the test rather than on the implementation.
     *
     * @param BasePackage[] $packages
     * @return string[]
     */
    private function names(array $packages): array
    {
        $names = [];
        foreach ($packages as $package) {
            if (strpos($package->getName(), '/') !== false) {
                $names[] = $package->getPrettyName() . '@' . $package->getPrettyVersion();
            }
        }
        sort($names);

        return $names;
    }
}
