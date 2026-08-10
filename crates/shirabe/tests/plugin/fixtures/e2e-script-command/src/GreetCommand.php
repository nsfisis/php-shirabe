<?php

namespace ShirabeTest\ScriptCommand;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

class GreetCommand extends Command
{
    protected function configure(): void
    {
        $this
            ->setDescription('Greets someone from a script-provided command.')
            ->setHelp('The <info>greet</info> command exercises a Command class named by a composer.json script.')
            ->addArgument('who', InputArgument::REQUIRED, 'Who to greet')
            ->addOption('shout', 's', InputOption::VALUE_NONE, 'Uppercase the greeting');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $message = 'Hello ' . $input->getArgument('who');
        if ($input->getOption('shout')) {
            $message = strtoupper($message);
        }
        $output->writeln('greet: ' . $message);
        $output->writeln('greet: name=' . $this->getName());
        $output->writeln('greet: app=' . get_class($this->getApplication()));

        return 0;
    }
}
