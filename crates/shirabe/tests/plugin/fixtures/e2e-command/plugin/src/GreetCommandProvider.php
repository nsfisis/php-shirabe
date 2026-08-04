<?php

namespace ShirabeTest\CommandProvider;

use Composer\Plugin\Capability\CommandProvider as CommandProviderCapability;

class GreetCommandProvider implements CommandProviderCapability
{
    public function getCommands(): array
    {
        return [new GreetCommand()];
    }
}
