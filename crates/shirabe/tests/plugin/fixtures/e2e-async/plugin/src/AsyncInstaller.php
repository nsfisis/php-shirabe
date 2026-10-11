<?php

namespace ShirabeTest\Async;

use Composer\Composer;
use Composer\Installer\InstallerInterface;
use Composer\Package\PackageInterface;
use Composer\Repository\InstalledRepositoryInterface;
use React\Promise\PromiseInterface;

/**
 * Installs `shirabe-async-asset` packages through promises that have not settled when the
 * installer hands them back: each step is a child process started on the executor of the run's
 * own Loop, so the caller has to drive that job before the step counts as done.
 */
class AsyncInstaller implements InstallerInterface
{
    /** @var Composer */
    private $composer;

    public function __construct(Composer $composer)
    {
        $this->composer = $composer;
    }

    public function supports(string $packageType): bool
    {
        return $packageType === 'shirabe-async-asset';
    }

    public function isInstalled(InstalledRepositoryInterface $repo, PackageInterface $package): bool
    {
        return is_file($this->getInstallPath($package) . '/asset.txt');
    }

    public function download(PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        // Composer downloads every package before it installs the plugin that registers this
        // installer, so a fresh install never gets here.
        return null;
    }

    public function prepare(string $type, PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        // A promise that settled before it was returned takes the same route.
        $this->trace('prepare returned');

        return \React\Promise\resolve(null);
    }

    public function install(InstalledRepositoryInterface $repo, PackageInterface $package): ?PromiseInterface
    {
        $path = $this->getInstallPath($package);

        return $this->step('install', 'sleep 0.2; echo unpacked')->then(function () use ($path, $package): void {
            @mkdir($path, 0777, true);
            file_put_contents($path . '/asset.txt', $package->getPrettyName() . "\n");
            $this->trace('install wrote asset');
        });
    }

    public function update(InstalledRepositoryInterface $repo, PackageInterface $initial, PackageInterface $target): ?PromiseInterface
    {
        return $this->install($repo, $target);
    }

    public function uninstall(InstalledRepositoryInterface $repo, PackageInterface $package): ?PromiseInterface
    {
        return null;
    }

    public function cleanup(string $type, PackageInterface $package, ?PackageInterface $prevPackage = null): ?PromiseInterface
    {
        $this->trace('cleanup');

        return null;
    }

    public function getInstallPath(PackageInterface $package): string
    {
        return 'assets/' . $package->getPrettyName();
    }

    private function step(string $name, string $command): PromiseInterface
    {
        $settled = false;
        $promise = $this->composer->getLoop()->getProcessExecutor()->executeAsync($command)
            ->then(function ($process) use ($name, &$settled): void {
                $settled = true;
                $this->trace($name . ' settled output=' . json_encode($process->getOutput()));
            });
        // The child has only just been started, so the step is still pending here.
        $this->trace($name . ' returned settled=' . json_encode($settled));

        return $promise;
    }

    private function trace(string $line): void
    {
        file_put_contents('async-installer-trace.txt', $line . "\n", FILE_APPEND);
    }
}
