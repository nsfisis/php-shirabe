<?php

namespace Shirabe;

/**
 * Raised by the guard classes (php/guards/): classes whose entity lives on the Rust side but that
 * this worker has no proxy for. Their declaration shadows the real Composer one, so running the
 * real implementation here — on an instance the Rust side never sees — fails loudly instead.
 */
final class UnsupportedClass
{
    public static function fail(string $class, string $member): void
    {
        throw new \RuntimeException(
            "{$class}::{$member}() is not available in the plugin runtime: the Rust side owns "
            . 'this class and there is no proxy for it here, so the real implementation would '
            . 'run on an instance the Rust side never sees.'
        );
    }
}
