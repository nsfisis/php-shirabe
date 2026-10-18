<?php

// Shirabe's own definition of Composer\Plugin\PreCommandRunEvent for the plugin worker, dual-mode
// like its parent Composer\EventDispatcher\Event: an instance revived from a Rust handle (the
// event a built-in command dispatches on the Rust side) forwards every call over RPC, while an
// instance constructed natively — real Composer code in this process does
// `new PreCommandRunEvent(...)` in BaseCommand::initialize() — is a faithful in-process port of
// the real class.

namespace Composer\Plugin;

use Composer\EventDispatcher\Event;
use Symfony\Component\Console\Input\InputInterface;

class PreCommandRunEvent extends Event
{
    /**
     * @var InputInterface (native mode)
     */
    private $input;

    /**
     * @var string (native mode)
     */
    private $command;

    public function __construct(string $name, InputInterface $input, string $command)
    {
        parent::__construct($name);
        $this->input = $input;
        $this->command = $command;
    }

    public function getInput(): InputInterface
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getInput', []);
        }

        return $this->input;
    }

    public function getCommand(): string
    {
        if ($this->__rhandle !== null) {
            return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getCommand', []);
        }

        return $this->command;
    }
}
