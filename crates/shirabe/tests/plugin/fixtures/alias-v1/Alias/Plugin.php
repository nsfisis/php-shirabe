<?php

namespace Alias;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Package\AliasPackage;
use Composer\Package\CompleteAliasPackage;
use Composer\Package\RootAliasPackage;
use Composer\Plugin\PluginInterface;

class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io)
    {
        $aliased = null;
        foreach ($composer->getRepositoryManager()->getLocalRepository()->getPackages() as $package) {
            if (!$package instanceof AliasPackage) {
                continue;
            }
            $aliased = $package->getAliasOf();
            $io->write('alias: ' . self::describe($package));
            $io->write('alias description: ' . $package->getDescription());
            $io->write('alias self-version: ' . ($package->hasSelfVersionRequires() ? 'yes' : 'no'));
            $io->write('alias root-flag: ' . ($package->isRootPackageAlias() ? 'yes' : 'no'));
            $package->setRootPackageAlias(true);
            $io->write('alias root-flag: ' . ($package->isRootPackageAlias() ? 'yes' : 'no'));
        }

        $root = $composer->getPackage();
        if (!$root instanceof RootAliasPackage) {
            throw new \RuntimeException('not a RootAliasPackage: ' . get_class($root));
        }
        $io->write('root: ' . self::describe($root));
        $io->write('root aliasOf identity: ' . ($root->getAliasOf() === $root->getAliasOf() ? 'same' : 'distinct'));
        $io->write('root minimum stability: ' . $root->getMinimumStability());
        $root->setMinimumStability('dev');
        $root->setExtra(['seen-by' => 'alias-v1']);

        $built = new CompleteAliasPackage($aliased, '9.9.9.9', '9.9.9');
        $io->write('built: ' . self::describe($built));
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
    }

    private static function describe(AliasPackage $package): string
    {
        return sprintf(
            '%s %s %s of %s %s',
            get_class($package),
            $package->getName(),
            $package->getPrettyVersion(),
            get_class($package->getAliasOf()),
            $package->getAliasOf()->getPrettyVersion()
        );
    }
}
