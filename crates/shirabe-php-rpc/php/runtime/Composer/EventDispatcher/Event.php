<?php

// Shirabe's own definition of Composer\EventDispatcher\Event for the plugin worker. This class
// lives in both worlds at once: an instance revived from a Rust handle proxies every call over
// RPC (like a generated stub), while an instance constructed natively — real Composer code in
// this process does `new PreCommandRunEvent(...)`, whose parent constructor lands here — is a
// faithful in-process port of the real base class. Proxy revival binds the handle without
// running the constructor, so the constructor below is the native mode alone;
// `__shirabeRustHandleDescriptor()` returns null there so the wire codec registers the object in
// the P table instead of treating it as a Rust handle.

namespace Composer\EventDispatcher;

class Event implements \ShirabeRustStub
{
    /** @var int|null Null in native mode. */
    protected $__rhandle;
    /** @var int */
    protected $__epoch = 0;

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
        $this->__rhandle = null;
        $this->name = $name;
        $this->args = $args;
        $this->flags = $flags;
    }

    public function __shirabeBind(int $rhandle, int $epoch): void
    {
        $this->__rhandle = $rhandle;
        $this->__epoch = $epoch;
    }

    public function __destruct()
    {
        if ($this->__rhandle !== null) {
            \ShirabeRustObjectRegistry::release($this->__rhandle);
        }
    }

    public function __shirabeRustHandleDescriptor(): ?array
    {
        if ($this->__rhandle === null) {
            return null;
        }

        return [
            '__rhandle' => $this->__rhandle,
            '__class' => static::class,
            '__epoch' => $this->__epoch,
        ];
    }

    public function getName(): string
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getName', []);
        }

        return $this->name;
    }

    public function getArguments(): array
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getArguments', []);
        }

        return $this->args;
    }

    public function getFlags(): array
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getFlags', []);
        }

        return $this->flags;
    }

    public function isPropagationStopped(): bool
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isPropagationStopped', []);
        }

        return $this->propagationStopped;
    }

    public function stopPropagation(): void
    {
        if ($this->__rhandle !== null) {
            \ShirabeRpcRuntime::callRust($this->__rhandle, 'stopPropagation', []);

            return;
        }
        $this->propagationStopped = true;
    }
}
