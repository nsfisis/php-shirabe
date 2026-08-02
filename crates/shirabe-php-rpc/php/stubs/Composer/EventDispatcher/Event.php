<?php

// Hand-written proxy stub for Composer\EventDispatcher\Event, kept in the shape the future stub
// generator will output: the real public methods, each forwarding to the Rust-side entity.

namespace Composer\EventDispatcher;

class Event implements \ShirabeRustStub
{
    /** @var int */
    protected $__rhandle;
    /** @var int */
    protected $__epoch;

    public function __construct(int $rhandle, int $epoch)
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

    public function getName(): string
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getName', []);
    }

    public function getArguments(): array
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getArguments', []);
    }

    public function getFlags(): array
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getFlags', []);
    }

    public function isPropagationStopped(): bool
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isPropagationStopped', []);
    }

    public function stopPropagation(): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'stopPropagation', []);
    }
}
