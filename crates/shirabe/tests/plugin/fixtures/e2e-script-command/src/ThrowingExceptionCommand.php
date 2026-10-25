<?php

namespace ShirabeTest\ScriptCommand;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class ThrowingExceptionCommand extends Command
{
    protected function configure(): void
    {
        $this->setDescription('Throws an exception from a script-provided command.');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln('throw-exception: before');

        throw new \RuntimeException('thrown by the command');
    }
}
