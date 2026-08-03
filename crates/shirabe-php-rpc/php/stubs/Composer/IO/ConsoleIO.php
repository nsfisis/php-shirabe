<?php

// Hand-written proxy stub for Composer\IO\ConsoleIO, kept in the shape the future stub
// generator will output. The IOInterface surface is inherited from the BaseIO stub; only the
// public methods ConsoleIO adds are declared here.

namespace Composer\IO;

use Symfony\Component\Console\Helper\Table;

class ConsoleIO extends BaseIO
{
    public function enableDebugging(float $startTime)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'enableDebugging', [$startTime]);
    }

    public function getProgressBar(int $max = 0)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getProgressBar', [$max]);
    }

    public function getTable(): Table
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getTable', []);
    }
}
