<?php

namespace ShirabeTest\AutoloadGenerator;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Package\Package;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

/**
 * Drives the AutoloadGenerator that Composer::getAutoloadGenerator() hands out and appends what
 * every call reports to autoload-generator-trace.txt, so the surface can be compared line by line
 * between implementations: the package map, the parsed autoloads with and without the dev
 * packages, a class loader built from them, the setters, and dry-run dumps with the class map
 * they return.
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
        // Whether an install resolves or replays a lock file decides which of the two fires, so
        // both are subscribed and the trace records the one that ran.
        return [
            ScriptEvents::POST_INSTALL_CMD => 'onPostCommand',
            ScriptEvents::POST_UPDATE_CMD => 'onPostCommand',
        ];
    }

    public function onPostCommand(Event $event): void
    {
        $composer = $this->composer;
        $generator = $composer->getAutoloadGenerator();
        $installationManager = $composer->getInstallationManager();
        $localRepo = $composer->getRepositoryManager()->getLocalRepository();
        $rootPackage = $composer->getPackage();
        $config = $composer->getConfig();

        $lines = ['event=' . $event->getName()];
        $lines[] = 'class=' . get_class($generator)
            . ' same=' . json_encode($generator === $composer->getAutoloadGenerator());

        $packageMap = $generator->buildPackageMap(
            $installationManager,
            $rootPackage,
            $localRepo->getCanonicalPackages()
        );
        $entries = [];
        foreach ($packageMap as [$package, $installPath]) {
            $entries[] = $package->getName() . '=' . json_encode($installPath === null ? null : $this->relative($installPath));
        }
        $lines[] = 'packageMap ' . implode(' ', $entries);
        $lines[] = 'packageMap rootIdentity=' . json_encode($packageMap[0][0] === $rootPackage);

        $invalid = new Package('acme/invalid', '1.0.0.0', '1.0.0');
        $invalid->setAutoload(['psr-4' => ['Acme\\Invalid' => 'src/']]);
        try {
            $generator->buildPackageMap($installationManager, $rootPackage, [$invalid]);
            $lines[] = 'invalid returned';
        } catch (\Exception $e) {
            $lines[] = 'invalid caught=' . get_class($e) . ' message=' . json_encode($e->getMessage());
        }

        $autoloads = $generator->parseAutoloads($packageMap, $rootPackage);
        $lines[] = 'autoloads ' . $this->relative(json_encode($autoloads));
        $lines[] = 'autoloads no-dev ' . $this->relative(json_encode(
            $generator->parseAutoloads($packageMap, $rootPackage, true)
        ));
        $lines[] = 'autoloads filtered ' . $this->relative(json_encode(
            $generator->parseAutoloads($packageMap, $rootPackage, ['acme/lib-b'])
        ));

        $vendorDir = $config->get('vendor-dir');
        $loader = $generator->createLoader($autoloads, $vendorDir);
        $lines[] = 'loader class=' . get_class($loader)
            . ' registered=' . json_encode(isset(\Composer\Autoload\ClassLoader::getRegisteredLoaders()[$vendorDir])
                && \Composer\Autoload\ClassLoader::getRegisteredLoaders()[$vendorDir] === $loader);
        $lines[] = 'loader psr-0 ' . $this->relative(json_encode($loader->getPrefixes()));
        $lines[] = 'loader psr-4 ' . $this->relative(json_encode($loader->getPrefixesPsr4()));
        $lines[] = 'loader classmap ' . $this->relative(json_encode($loader->getClassMap()));
        $lines[] = 'loader findFile ' . $this->relative(json_encode($loader->findFile('Acme\\A\\Widget')));

        try {
            $generator->createLoader(['psr-4' => ['Acme\\Invalid' => ['src/']]]);
            $lines[] = 'invalid loader returned';
        } catch (\Exception $e) {
            $lines[] = 'invalid loader caught=' . get_class($e) . ' message=' . json_encode($e->getMessage());
        }

        $lines[] = 'setters ' . json_encode([
            $generator->setDevMode(true),
            $generator->setApcu(false),
            $generator->setRunScripts(false),
            $generator->setIgnorePlatformRequirements(false),
            $generator->setClassMapAuthoritative(false),
        ]);
        $generator->setDryRun(true);

        $classMap = $generator->dump($config, $localRepo, $rootPackage, $installationManager, 'composer', false, 'Probe');
        $lines[] = 'dump class=' . get_class($classMap) . ' count=' . count($classMap);
        $lines[] = 'dump map ' . $this->relative(json_encode($classMap->getMap()));
        $lines[] = 'dump ambiguous ' . $this->relative(json_encode($classMap->getAmbiguousClasses(false)));
        $lines[] = 'dump psrViolations ' . $this->relative(json_encode($classMap->getPsrViolations()));

        $classMap = $generator->dump($config, $localRepo, $rootPackage, $installationManager, 'composer', true, 'Probe');
        $lines[] = 'dump scanPsr map ' . $this->relative(json_encode($classMap->getMap()));
        $lines[] = 'dump scanPsr psrViolations ' . $this->relative(json_encode($classMap->getPsrViolations()));

        // An authoritative class map forces the PSR scan on, so the dump has to see the setter.
        $generator->setClassMapAuthoritative(true);
        $classMap = $generator->dump($config, $localRepo, $rootPackage, $installationManager, 'composer', false, 'Probe');
        $lines[] = 'dump authoritative count=' . count($classMap);

        $generator->setDevMode(false);
        $classMap = $generator->dump($config, $localRepo, $rootPackage, $installationManager, 'composer', true, 'Probe');
        $lines[] = 'dump no-dev map ' . $this->relative(json_encode(array_keys($classMap->getMap())));

        $generator->setClassMapAuthoritative(false);
        $generator->setDryRun(false);

        file_put_contents('autoload-generator-trace.txt', implode("\n", $lines) . "\n", FILE_APPEND);
    }

    /**
     * Paths come back absolute, and the working directory differs between runs. The directory is
     * replaced both as written and as json_encode() escapes it.
     */
    private function relative(string $text): string
    {
        $cwd = (string) getcwd();

        return str_replace([$cwd, trim((string) json_encode($cwd), '"')], '<cwd>', $text);
    }
}
