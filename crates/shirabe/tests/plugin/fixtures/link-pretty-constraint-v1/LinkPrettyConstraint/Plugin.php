<?php

namespace LinkPrettyConstraint;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Package\Link;
use Composer\Plugin\PluginInterface;
use Composer\Semver\Constraint\MatchAllConstraint;

class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io)
    {
        $package = $composer->getPackage();

        foreach ($package->getRequires() as $name => $link) {
            try {
                $io->write($name . ': ' . $link->getPrettyConstraint());
            } catch (\UnexpectedValueException $e) {
                $io->write($name . ': ' . get_class($e) . ': ' . $e->getMessage());
            }
        }

        $package->setRequires([
            'bar/unset' => new Link('dummy/root', 'bar/unset', new MatchAllConstraint()),
        ]);
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
    }
}
