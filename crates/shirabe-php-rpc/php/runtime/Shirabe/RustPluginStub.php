<?php

// Proxy stub for a plugin whose implementation lives on the Rust side. It has no counterpart
// class in Composer: a plugin is normally PHP code running in this process, and only a
// Rust-implemented one needs a stand-in here (a capability constructor receives the plugin
// itself as $ctorArgs['plugin']).

namespace Shirabe;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

class RustPluginStub implements PluginInterface, \ShirabeRustStub
{
    /** @var int */
    protected $__rhandle;
    /** @var int */
    protected $__epoch;

    /**
     * Binds a stub the registry built for an existing entity. Proxy instantiation bypasses
     * the constructor, which belongs to plugin code building a new entity instead.
     */
    public function __shirabeBind(int $rhandle, int $epoch): void
    {
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

    public function __clone()
    {
        // PHP has already shallow-copied this stub, so both copies would point at one
        // entity and release it twice. The Rust side clones the entity instead, applying
        // whatever __clone semantics the real class defines, and this copy rebinds to the
        // fresh handle. Entities without clone semantics answer with an explicit error.
        [$this->__rhandle, $this->__epoch] = \ShirabeRpcRuntime::callRust($this->__rhandle, '__shirabeClone', []);
        \ShirabeRustObjectRegistry::adopt($this->__rhandle, $this);
    }

    public function __get($name)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, '__get', [$name]);
    }

    public function __set($name, $value): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, '__set', [$name, $value]);
    }

    public function __isset($name): bool
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, '__isset', [$name]);
    }

    public function __unset($name): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, '__unset', [$name]);
    }

    public function activate(Composer $composer, IOInterface $io)
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'activate', [$composer, $io]);
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'deactivate', [$composer, $io]);
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'uninstall', [$composer, $io]);
    }
}
