<?php

namespace ShirabeTest\CommandProvider;

use Composer\Command\BaseCommand;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

class GreetCommand extends BaseCommand
{
    protected function configure(): void
    {
        $this
            ->setName('greet')
            ->setAliases(['hi'])
            ->setDescription('Greets someone from a plugin-provided command.')
            ->setHelp('The <info>greet</info> command exercises plugin-provided command execution.')
            ->addArgument('who', InputArgument::REQUIRED, 'Who to greet')
            ->addOption('shout', 's', InputOption::VALUE_NONE, 'Uppercase the greeting')
            ->addOption('out', null, InputOption::VALUE_REQUIRED, 'File the command writes its observations to');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $message = 'Hello ' . $input->getArgument('who');
        if ($input->getOption('shout')) {
            $message = strtoupper($message);
        }
        $this->getIO()->write('greet: ' . $message);

        $composer = $this->requireComposer();
        $lines = [
            'root=' . $composer->getPackage()->getName(),
            'app=' . get_class($this->getApplication()),
        ];
        $aboutExit = $this->getApplication()->find('about')->run(new ArrayInput(['command' => 'about']), $output);
        $lines[] = 'about=' . $aboutExit;
        $out = $input->getOption('out');
        if ($out !== null) {
            file_put_contents($out, implode("\n", $lines) . "\n");
        }

        return 0;
    }
}
