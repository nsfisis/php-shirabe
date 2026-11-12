<?php

namespace Shirabe;

/** Interns proxy stubs so the same Rust handle always yields the same stub instance. */
final class RustObjectRegistry
{
    /** @var array<int, WeakReference> */
    private static array $internTable = [];

    public static function stubFor(int $rhandle, string $class): object
    {
        if (isset(self::$internTable[$rhandle])) {
            $existing = self::$internTable[$rhandle]->get();
            if ($existing !== null) {
                return $existing;
            }
        }
        // A guard class exists for every proxied FQCN without a stub, so class_exists() alone no
        // longer tells the two apart; only a stub can be bound to a Rust handle.
        if (!class_exists($class) || !is_a($class, RustStub::class, true)) {
            throw new \RuntimeException(
                "no proxy stub class is available for {$class}"
            );
        }
        // The stub's constructor belongs to plugin code building a *new* entity; an entity that
        // already exists is bound directly, so proxying never runs it.
        $stub = (new \ReflectionClass($class))->newInstanceWithoutConstructor();
        $stub->__shirabeBind($rhandle);
        self::$internTable[$rhandle] = \WeakReference::create($stub);
        return $stub;
    }

    /**
     * Interns a stub the registry did not build: a `__clone` forwarder rebinds the copy PHP
     * made to a freshly cloned entity, and that pairing has to be visible to later crossings
     * of the same handle.
     */
    public static function adopt(int $rhandle, object $stub): void
    {
        self::$internTable[$rhandle] = \WeakReference::create($stub);
    }

    /** Invoked from stub destructors. */
    public static function release(int $rhandle): void
    {
        unset(self::$internTable[$rhandle]);
        RpcRuntime::notifyReleaseHandle($rhandle);
    }
}
