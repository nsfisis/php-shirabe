<?php

// The PHP half of the by-value object codec (crates/shirabe/src/php_worker/codec.rs).
// An object whose entity lives on the Rust side crosses as a handle, but an immutable value
// has no entity to point at: the child holds a genuine instance of the real class instead, and
// a frame carries the object record `serialize()` writes for it, which `unserialize()` revives
// without running a constructor.
//
// CLASSES is the whole vocabulary that may cross this way: it is both the set of classes handed
// to `serialize()` in place of a handle, and, together with the handle classes,
// the `allowed_classes` list every frame payload is unserialized under
// (`Shirabe\RpcRuntime::unserializePayload`), so a payload can never name another class. It is
// generated from the classifier report (Shirabe\ByValueClasses).

namespace Shirabe;

final class ByValueObject
{
    public const CLASSES = ByValueClasses::CLASSES;

    /**
     * The object to serialize into a frame payload in place of $value, or null when the Rust side
     * has no value to rebuild it as (it then crosses as a PHP entity).
     */
    public static function of(object $value): ?object
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
