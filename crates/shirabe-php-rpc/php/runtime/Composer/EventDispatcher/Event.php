<?php

// ref: composer/src/Composer/EventDispatcher/Event.php sha1:bba4e2fefb94fd89492284ee0fe42112847e6608
//
// Shirabe's own definition of Composer\EventDispatcher\Event for the plugin worker. This class
// lives on both sides at once: an instance revived from a Rust entity handle proxies every call
// over RPC (like a generated stub), while an instance constructed natively — real Composer code in
// this process does `new PreCommandRunEvent(...)`, whose parent constructor lands here — is a
// faithful in-process port of the real base class. Proxy revival binds the handle without running
// the constructor, so the constructor below is the native mode alone; `__shirabeRustEntityHandle()`
// returns null there so `Shirabe\RpcRuntime::toRpcValue` registers the object in the PHP entity
// table instead of treating it as a Rust entity.

namespace Composer\EventDispatcher;

class Event implements \Shirabe\RustStub
{
    /** @var int|null Null in native mode. */
    protected $__entityId;
    /** @var string This event's name (native mode) */
    protected $name;

    /** @var string[] Arguments passed by the user, these will be forwarded to CLI script handlers (native mode) */
    protected $args;

    /** @var mixed[] Flags usable in PHP script handlers (native mode) */
    protected $flags;

    /** @var bool Whether the event should not be passed to more listeners (native mode) */
    private $propagationStopped = false;

    public function __construct(string $name, array $args = [], array $flags = [])
    {
        $this->__entityId = null;
        $this->name = $name;
        $this->args = $args;
        $this->flags = $flags;
    }

    public function __shirabeBind(int $entityId): void
    {
        $this->__entityId = $entityId;
    }

    public function __destruct()
    {
        if ($this->__entityId !== null) {
            \Shirabe\RustStubRegistry::release($this->__entityId);
        }
    }

    public function __shirabeRustEntityHandle(): ?\Shirabe\Rpc\RustEntityHandle
    {
        if ($this->__entityId === null) {
            return null;
        }

        return new \Shirabe\Rpc\RustEntityHandle($this->__entityId, static::class);
    }

    public function getName(): string
    {
        if ($this->__entityId !== null) {
            return \Shirabe\RpcRuntime::callRust($this->__entityId, 'getName', []);
        }

        return $this->name;
    }

    public function getArguments(): array
    {
        if ($this->__entityId !== null) {
            return \Shirabe\RpcRuntime::callRust($this->__entityId, 'getArguments', []);
        }

        return $this->args;
    }

    public function getFlags(): array
    {
        if ($this->__entityId !== null) {
            return \Shirabe\RpcRuntime::callRust($this->__entityId, 'getFlags', []);
        }

        return $this->flags;
    }

    public function isPropagationStopped(): bool
    {
        if ($this->__entityId !== null) {
            return \Shirabe\RpcRuntime::callRust($this->__entityId, 'isPropagationStopped', []);
        }

        return $this->propagationStopped;
    }

    public function stopPropagation(): void
    {
        if ($this->__entityId !== null) {
            \Shirabe\RpcRuntime::callRust($this->__entityId, 'stopPropagation', []);

            return;
        }
        $this->propagationStopped = true;
    }
}
