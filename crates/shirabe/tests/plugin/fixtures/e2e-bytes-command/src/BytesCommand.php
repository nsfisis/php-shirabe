<?php

namespace ShirabeTest\BytesCommand;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

class BytesCommand extends Command
{
    protected function configure(): void
    {
        $this->setDescription("description-\xE9")
            ->setAliases(["alias\xE9", "namespace\xE9:sub"])
            ->setHelp("help-\xE9")
            ->addUsage("usage-\xE9")
            ->addArgument('arg', InputArgument::OPTIONAL, "argument-description-\xE9")
            ->addOption('opt', null, InputOption::VALUE_NONE, "option-description-\xE9");
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln('bytes: ran');

        return 0;
    }
}
