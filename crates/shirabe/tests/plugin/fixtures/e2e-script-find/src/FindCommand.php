<?php

namespace ShirabeTest\ScriptFind;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class FindCommand extends Command
{
    protected function configure(): void
    {
        $this->setDescription('Looks up the plain script command through the application.');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $application = $this->getApplication();
        $output->writeln('finder: has=' . var_export($application->has('plain'), true));

        $plain = $application->find('plain');
        $output->writeln('finder: name=' . $plain->getName());
        $output->writeln('finder: description=' . $plain->getDescription());

        $exitCode = $plain->run(new ArrayInput(['command' => 'plain']), $output);
        $output->writeln('finder: exit=' . $exitCode);

        return 0;
    }
}
