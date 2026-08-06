<?php

namespace Values;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Package\Link;
use Composer\Plugin\PluginInterface;
use Composer\Semver\Constraint\Constraint;
use Composer\Semver\Constraint\ConstraintInterface;
use Composer\Semver\Constraint\MatchAllConstraint;
use Composer\Semver\Constraint\MatchNoneConstraint;
use Composer\Semver\Constraint\MultiConstraint;

class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io)
    {
        $package = $composer->getPackage();

        foreach ($package->getRequires() as $name => $link) {
            if (!$link instanceof Link) {
                throw new \RuntimeException('not a Link: ' . get_class($link));
            }
            $io->write(sprintf(
                '%s | %s | %s | %s | %s | %s',
                $name,
                $link->getSource(),
                $link->getTarget(),
                $link->getDescription(),
                $link->getPrettyConstraint(),
                self::render($link->getConstraint())
            ));
        }

        $package->setRequires([
            'bar/plain' => new Link('dummy/root', 'bar/plain', self::pretty(new Constraint('==', '2.0.0.0'), '2.0'), Link::TYPE_REQUIRE, '2.0'),
            'bar/multi' => new Link('dummy/root', 'bar/multi', new MultiConstraint([
                new Constraint('<', '1.0.0.0'),
                new Constraint('>=', '3.0.0.0'),
            ], false), Link::TYPE_DEV_REQUIRE, '<1.0 || >=3.0'),
            'bar/all' => new Link('dummy/root', 'bar/all', new MatchAllConstraint(), Link::TYPE_REQUIRE, '*'),
            'bar/none' => new Link('dummy/root', 'bar/none', self::pretty(new MatchNoneConstraint(), 'nothing'), Link::TYPE_REQUIRE, ''),
        ]);

        $package->setReleaseDate(new \DateTimeImmutable('2024-03-04 05:06:07.123456', new \DateTimeZone('Asia/Tokyo')));
        $io->write('release date: ' . $package->getReleaseDate()->format('Y-m-d\TH:i:s.uP'));
    }

    public function deactivate(Composer $composer, IOInterface $io)
    {
    }

    public function uninstall(Composer $composer, IOInterface $io)
    {
    }

    private static function pretty(ConstraintInterface $constraint, string $prettyString): ConstraintInterface
    {
        $constraint->setPrettyString($prettyString);

        return $constraint;
    }

    private static function render(ConstraintInterface $constraint): string
    {
        if ($constraint instanceof Constraint) {
            $shape = sprintf('Constraint(%s %s)', $constraint->getOperator(), $constraint->getVersion());
        } elseif ($constraint instanceof MultiConstraint) {
            $shape = sprintf(
                'MultiConstraint(%s: %s)',
                $constraint->isConjunctive() ? 'and' : 'or',
                implode(', ', array_map([self::class, 'render'], $constraint->getConstraints()))
            );
        } elseif ($constraint instanceof MatchAllConstraint) {
            $shape = 'MatchAllConstraint()';
        } elseif ($constraint instanceof MatchNoneConstraint) {
            $shape = 'MatchNoneConstraint()';
        } else {
            throw new \RuntimeException('unexpected constraint: ' . get_class($constraint));
        }

        return $shape . ' pretty=' . $constraint->getPrettyString();
    }
}
