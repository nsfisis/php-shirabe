<?php

namespace ShirabeTest\ScriptCommand;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class MismatchedNameCommand extends Command
{
    protected function configure(): void
    {
        $this->setName('not-the-script-name');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln('renamed: name=' . $this->getName());
        $output->writeln('renamed: description=' . $this->getDescription());

        return 0;
    }
}
