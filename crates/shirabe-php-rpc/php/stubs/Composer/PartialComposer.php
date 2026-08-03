<?php

// Hand-written proxy stub for Composer\PartialComposer, kept in the shape the future stub
// generator will output: the real public methods, each forwarding to the Rust-side entity.
// Methods the Rust dispatcher does not support yet surface as explicit RPC errors.

namespace Composer;

use Composer\Autoload\AutoloadGenerator;
use Composer\Config;
use Composer\EventDispatcher\EventDispatcher;
use Composer\Installer\InstallationManager;
use Composer\Package\RootPackageInterface;
use Composer\Repository\RepositoryManager;
use Composer\Util\Loop;

class PartialComposer implements \ShirabeRustStub
{
    /** @var int */
    protected $__rhandle;
    /** @var int */
    protected $__epoch;

    public function __construct(int $rhandle = 0, int $epoch = 0)
    {
        if (func_num_args() < 2) {
            // Constructing the class from plugin code (a common idiom for e.g. `new BufferIO()`)
            // is an open question of the plugin design; only proxy instantiation passes a
            // Rust handle. Fail with a diagnosable message instead of an ArgumentCountError.
            throw new \RuntimeException(
                'Shirabe does not support constructing ' . static::class . ' inside the plugin process yet'
            );
        }
        $this->__rhandle = $rhandle;
        $this->__epoch = $epoch;
    }

    public function __destruct()
    {
        \ShirabeRustObjectRegistry::release($this->__rhandle);
    }

    public function __shirabeRustHandleDescriptor(): array
    {
        return [
            '__rhandle' => $this->__rhandle,
            '__class' => static::class,
            '__epoch' => $this->__epoch,
        ];
    }

    public function setPackage(RootPackageInterface $package): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setPackage', [$package]);
    }

    public function getPackage(): RootPackageInterface
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getPackage', []);
    }

    public function setConfig(Config $config): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setConfig', [$config]);
    }

    public function getConfig(): Config
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getConfig', []);
    }

    public function setLoop(Loop $loop): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setLoop', [$loop]);
    }

    public function getLoop(): Loop
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getLoop', []);
    }

    public function setRepositoryManager(RepositoryManager $manager): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setRepositoryManager', [$manager]);
    }

    public function getRepositoryManager(): RepositoryManager
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getRepositoryManager', []);
    }

    public function setInstallationManager(InstallationManager $manager): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setInstallationManager', [$manager]);
    }

    public function getInstallationManager(): InstallationManager
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getInstallationManager', []);
    }

    public function setEventDispatcher(EventDispatcher $eventDispatcher): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setEventDispatcher', [$eventDispatcher]);
    }

    public function getEventDispatcher(): EventDispatcher
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getEventDispatcher', []);
    }

    public function isGlobal(): bool
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isGlobal', []);
    }

    public function setGlobal(): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setGlobal', []);
    }
}
