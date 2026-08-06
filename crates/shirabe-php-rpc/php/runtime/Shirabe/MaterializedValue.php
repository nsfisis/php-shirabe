<?php

// The PHP half of the materialized-value codec (crates/shirabe/src/plugin/php_plugin_value.rs).
// An object whose entity lives on the Rust side crosses as a handle, but an immutable value
// has no entity to point at: the child holds a genuine instance of the real class instead, and
// the wire carries the class name plus the constructor arguments needed to rebuild it.
//
// Only the classes listed in CLASSES cross this way. An unknown class name is an explicit
// error rather than a `new $class`, so the descriptor can never name an arbitrary class.

namespace Shirabe;

use Composer\Package\Link;
use Composer\Semver\Constraint\Constraint;
use Composer\Semver\Constraint\MatchAllConstraint;
use Composer\Semver\Constraint\MatchNoneConstraint;
use Composer\Semver\Constraint\MultiConstraint;

final class MaterializedValue
{
    private const CLASSES = [
        Link::class,
        Constraint::class,
        MultiConstraint::class,
        MatchAllConstraint::class,
        MatchNoneConstraint::class,
        \DateTimeImmutable::class,
        \DateTime::class,
    ];

    /**
     * @param array{__pnew: string, __args?: array, __calls?: array} $descriptor
     */
    public static function build(array $descriptor): object
    {
        $class = $descriptor['__pnew'];
        if (!\in_array($class, self::CLASSES, true)) {
            throw new \RuntimeException(
                "the class {$class} cannot be materialized in the plugin runtime"
            );
        }
        $value = new $class(...array_values($descriptor['__args'] ?? []));
        foreach ($descriptor['__calls'] ?? [] as [$method, $args]) {
            $value->$method(...array_values($args));
        }
        return $value;
    }

    /**
     * The descriptor for a value the Rust side rebuilds by value, or null when the object is
     * not one of them (it then crosses as a P-table entity).
     *
     * @return ?array{__pnew: string, __args: array, __calls?: array}
     */
    public static function describe(object $value): ?array
    {
        if ($value instanceof Link) {
            return [
                '__pnew' => Link::class,
                '__args' => [
                    self::field($value, 'source'),
                    self::field($value, 'target'),
                    $value->getConstraint(),
                    self::field($value, 'description'),
                    // Not getPrettyConstraint(): that throws when the link was built without
                    // one, and an absent pretty constraint has to cross as absent.
                    self::field($value, 'prettyConstraint'),
                ],
            ];
        }
        if ($value instanceof Constraint) {
            return self::constraint($value, [$value->getOperator(), $value->getVersion()]);
        }
        if ($value instanceof MultiConstraint) {
            return self::constraint($value, [$value->getConstraints(), $value->isConjunctive()]);
        }
        if ($value instanceof MatchAllConstraint || $value instanceof MatchNoneConstraint) {
            return self::constraint($value, []);
        }
        if ($value instanceof \DateTimeInterface) {
            return [
                '__pnew' => $value instanceof \DateTime ? \DateTime::class : \DateTimeImmutable::class,
                // DATE_ATOM widened by the microseconds a PHP date carries, so the instant
                // crosses at the full precision this side can represent.
                '__args' => [$value->format('Y-m-d\TH:i:s.uP')],
            ];
        }
        return null;
    }

    /**
     * No constraint constructor takes the pretty string, and whether it was ever set is
     * observable through getPrettyString(), so it travels as a post-construction call.
     *
     * @param list<mixed> $args
     * @return array{__pnew: string, __args: list<mixed>, __calls: list<array{string, list<mixed>}>}
     */
    private static function constraint(object $value, array $args): array
    {
        return [
            '__pnew' => \get_class($value),
            '__args' => $args,
            '__calls' => [['setPrettyString', [self::field($value, 'prettyString')]]],
        ];
    }

    /** Reads a protected field these value classes expose no getter for. */
    private static function field(object $value, string $name)
    {
        $property = new \ReflectionProperty($value, $name);
        (\PHP_VERSION_ID < 80100) and $property->setAccessible(true);
        return $property->getValue($value);
    }
}
