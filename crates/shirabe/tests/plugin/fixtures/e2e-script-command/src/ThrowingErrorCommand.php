<?php

namespace ShirabeTest\ScriptCommand;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class ThrowingErrorCommand extends Command
{
    protected function configure(): void
    {
        $this->setDescription('Throws an \Error from a script-provided command.');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln('throw-error: before');

        throw new \Error('raised by the command');
    }
}
