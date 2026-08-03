<?php

// Hand-written proxy stub for Composer\IO\BufferIO, kept in the shape the future stub
// generator will output.

namespace Composer\IO;

class BufferIO extends ConsoleIO
{
    public function getOutput(): string
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getOutput', []);
    }

    public function setUserInputs(array $inputs): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setUserInputs', [$inputs]);
    }
}
