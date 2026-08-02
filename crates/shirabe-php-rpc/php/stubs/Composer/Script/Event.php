<?php

// Hand-written proxy stub for Composer\Script\Event, kept in the shape the future stub
// generator will output. See Composer/EventDispatcher/Event.php.

namespace Composer\Script;

use Composer\EventDispatcher\Event as BaseEvent;

class Event extends BaseEvent
{
    public function getComposer(): \Composer\Composer
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getComposer', []);
    }

    public function getIO(): \Composer\IO\IOInterface
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getIO', []);
    }

    public function isDevMode(): bool
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isDevMode', []);
    }

    public function getOriginatingEvent(): ?BaseEvent
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getOriginatingEvent', []);
    }

    public function setOriginatingEvent(BaseEvent $event): self
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setOriginatingEvent', [$event]);
        return $this;
    }
}
