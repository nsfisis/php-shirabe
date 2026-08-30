<?php

// The PHP half of the materialized-value codec (crates/shirabe/src/plugin/php_plugin_value.rs).
// An object whose entity lives on the Rust side crosses as a handle, but an immutable value
// has no entity to point at: the child holds a genuine instance of the real class instead, and
// the wire carries the object record `serialize()` writes for it, which `unserialize()` revives
// without running a constructor.
//
// CLASSES is the whole vocabulary that may cross this way: it is both the set of classes handed
// to `serialize()` in place of a handle descriptor, and the `allowed_classes` list every frame
// payload is unserialized under, so a payload can never name another class.

namespace Shirabe;

use Composer\Package\Link;
use Composer\Semver\Constraint\Constraint;
use Composer\Semver\Constraint\MatchAllConstraint;
use Composer\Semver\Constraint\MatchNoneConstraint;
use Composer\Semver\Constraint\MultiConstraint;
use Composer\Util\Http\Response;

final class MaterializedValue
{
    public const CLASSES = [
        Link::class,
        Constraint::class,
        MultiConstraint::class,
        MatchAllConstraint::class,
        MatchNoneConstraint::class,
        Response::class,
        \DateTimeImmutable::class,
        \DateTime::class,
    ];

    /**
     * The object to serialize onto the wire in place of $value, or null when the Rust side has
     * no value to rebuild it as (it then crosses as a P-table entity).
     */
    public static function forWire(object $value): ?object
    {
        if ($value instanceof \DateTimeInterface) {
            // The Rust side holds instants in UTC and carries no timezone database, so the date
            // crosses rebased on UTC. Going through the offset rather than the zone keeps the
            // instant exact across an ambiguous wall clock, and drops any subclass a plugin
            // brought along.
            return (new \DateTimeImmutable($value->format('Y-m-d H:i:s.uP')))
                ->setTimezone(new \DateTimeZone('UTC'));
        }

        // Not instanceof: a subclass has state and behaviour of its own that no Rust value
        // carries, so it crosses as an entity instead.
        return \in_array(\get_class($value), self::CLASSES, true) ? $value : null;
    }
}
