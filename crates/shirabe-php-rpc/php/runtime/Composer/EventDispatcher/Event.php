<?php

// Shirabe's own definition of Composer\EventDispatcher\Event for the plugin worker. This class
// lives in both worlds at once: an instance revived from a Rust handle proxies every call over
// RPC (like a generated stub), while an instance constructed natively — real Composer code in
// this process does `new PreCommandRunEvent(...)`, whose parent constructor lands here — is a
// faithful in-process port of the real base class. The two modes are told apart by the
// constructor arguments; `__shirabeRustHandleDescriptor()` returns null in native mode so the
// wire codec registers the object in the P table instead of treating it as a Rust handle.

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

    /**
     * Proxy revival passes (int $rhandle, int $epoch); the real class's constructor is
     * (string $name, array $args = [], array $flags = []).
     */
    public function __construct($name = null, $args = [], $flags = [])
    {
        if (is_int($name) && func_num_args() === 2 && is_int($args)) {
            $this->__rhandle = $name;
            $this->__epoch = $args;

            return;
        }
        if (!is_string($name)) {
            throw new \RuntimeException(
                'Shirabe does not support constructing ' . static::class . ' inside the plugin process without an event name'
            );
        }
        $this->__rhandle = null;
        $this->name = $name;
        $this->args = $args;
        $this->flags = $flags;
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
