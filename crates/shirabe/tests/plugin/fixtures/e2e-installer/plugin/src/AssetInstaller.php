<?php

namespace ShirabeTest\AssetInstaller;

use Composer\Composer;
use Composer\Installer\InstallerInterface;
use Composer\IO\IOInterface;
use Composer\Package\PackageInterface;
use Composer\Repository\InstalledRepositoryInterface;
use React\Promise\PromiseInterface;

/**
 * Installs `shirabe-asset` packages by writing a manifest of what the installer observed,
 * without going through the download manager. Every call also appends a trace line, so the
 * order and arguments of the installer contract are comparable between implementations.
 */
class AssetInstaller implements InstallerInterface
{
    /** @var IOInterface */
    private $io;

    /** @var Composer */
    private $composer;

    public function __construct(IOInterface $io, Composer $composer)
    {
        $this->io = $io;
        $this->composer = $composer;
    }

    public function supports(string $packageType): bool
    {
        $this->trace('supports ' . $packageType);

        return $packageType === 'shirabe-asset';
    }

    public function isInstalled(InstalledRepositoryInterface $repo, PackageInterface $package): bool
    {
        $this->trace('isInstalled ' . $package->getPrettyName());

        return is_file($this->getInstallPath($package) . '/asset.txt');
    }

    public function download(PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        $this->trace('download ' . $package->getPrettyName() . ' prev=' . $this->name($prevPackage));

        return null;
    }

    public function prepare(string $type, PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        $this->trace('prepare ' . $type . ' ' . $package->getPrettyName() . ' prev=' . $this->name($prevPackage));

        return null;
    }

    public function install(InstalledRepositoryInterface $repo, PackageInterface $package): ?PromiseInterface
    {
        $path = $this->getInstallPath($package);
        $this->trace('install ' . $package->getPrettyName() . ' -> ' . $path);
        @mkdir($path, 0777, true);
        file_put_contents($path . '/asset.txt', implode("\n", [
            'name=' . $package->getPrettyName(),
            'version=' . $package->getPrettyVersion(),
            'type=' . $package->getType(),
            'root=' . $this->composer->getPackage()->getName(),
        ]) . "\n");
        $this->io->write('asset-installer: installed ' . $package->getPrettyName());

        return null;
    }

    public function update(InstalledRepositoryInterface $repo, PackageInterface $initial, PackageInterface $target): ?PromiseInterface
    {
        $this->trace('update ' . $initial->getPrettyName() . ' -> ' . $target->getPrettyName());

        return $this->install($repo, $target);
    }

    public function uninstall(InstalledRepositoryInterface $repo, PackageInterface $package): ?PromiseInterface
    {
        $path = $this->getInstallPath($package);
        $this->trace('uninstall ' . $package->getPrettyName());
        @unlink($path . '/asset.txt');
        @rmdir($path);

        return null;
    }

    public function cleanup(string $type, PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        $this->trace('cleanup ' . $type . ' ' . $package->getPrettyName());

        return null;
    }

    public function getInstallPath(PackageInterface $package): string
    {
        return 'assets/' . $package->getPrettyName();
    }

    private function name(?PackageInterface $package): string
    {
        return $package === null ? 'null' : $package->getPrettyName();
    }

    private function trace(string $line): void
    {
        file_put_contents('installer-trace.txt', $line . "\n", FILE_APPEND);
    }
}
