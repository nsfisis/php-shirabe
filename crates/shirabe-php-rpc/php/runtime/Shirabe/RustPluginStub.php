<?php

// Proxy stub for a plugin whose implementation lives on the Rust side. It has no counterpart
// class in Composer: a plugin is normally PHP code running in this process, and only a
// Rust-implemented one needs a stand-in here (a capability constructor receives the plugin
// itself as $ctorArgs['plugin']).

namespace Shirabe;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

class RustPluginStub implements PluginInterface, \Shirabe\RustStub
{
    /** @var int */
    protected $__entityId;

    /**
     * Binds a stub the registry built for an existing entity. Proxy instantiation bypasses
     * the constructor, which belongs to plugin code building a new entity instead.
     */
    public function __shirabeBind(int $entityId): void
    {
        $this->__entityId = $entityId;
    }

    public function __destruct()
    {
        \Shirabe\RustStubRegistry::release($this->__entityId);
    }

    public function __shirabeRustEntityHandle(): \Shirabe\Rpc\RustEntityHandle
    {
        return new \Shirabe\Rpc\RustEntityHandle($this->__entityId, static::class);
    }

    public function __clone()
    {
        // PHP has already shallow-copied this stub, so both copies would point at one
        // entity and release it twice. The Rust side clones the entity instead, applying
        // whatever __clone semantics the real class defines, and this copy rebinds to the
        // fresh handle. Entities without clone semantics answer with an explicit error.
        $this->__entityId = \Shirabe\RpcRuntime::callRust($this->__entityId, '__shirabe_clone', []);
        \Shirabe\RustStubRegistry::adopt($this->__entityId, $this);
    }

    public function __get($name)
    {
        return \Shirabe\RpcRuntime::callRust($this->__entityId, '__get', [$name]);
    }

    public function __set($name, $value): void
    {
        \Shirabe\RpcRuntime::callRust($this->__entityId, '__set', [$name, $value]);
    }

    public function __isset($name): bool
    {
        return \Shirabe\RpcRuntime::callRust($this->__entityId, '__isset', [$name]);
    }

    public function __unset($name): void
    {
        \Shirabe\RpcRuntime::callRust($this->__entityId, '__unset', [$name]);
    }

    public function activate(Composer $composer, IOInterface $io)
    {
        \Shirabe\RpcRuntime::callRust($this->__entityId, 'activate', [$composer, $io]);
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
        \Shirabe\RpcRuntime::callRust($this->__entityId, 'deactivate', [$composer, $io]);
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
        \Shirabe\RpcRuntime::callRust($this->__entityId, 'uninstall', [$composer, $io]);
    }
}
