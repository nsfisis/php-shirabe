<?php

namespace Shirabe;

/** Interns proxy stubs so the same Rust entity id always yields the same stub instance. */
final class RustStubRegistry
{
    /** @var array<int, WeakReference> */
    private static array $internTable = [];

    public static function stubFor(int $entityId, string $class): object
    {
        if (isset(self::$internTable[$entityId])) {
            $existing = self::$internTable[$entityId]->get();
            if ($existing !== null) {
                return $existing;
            }
        }
        // A guard class exists for every proxied FQCN without a stub, so class_exists() alone no
        // longer tells the two apart; only a stub can be bound to a Rust entity id.
        if (!class_exists($class) || !is_a($class, RustStub::class, true)) {
            throw new \RuntimeException("no proxy stub class is available for {$class}");
        }
        // The stub's constructor belongs to plugin code building a *new* entity; an entity that
        // already exists is bound directly, so proxying never runs it.
        $stub = (new \ReflectionClass($class))->newInstanceWithoutConstructor();
        $stub->__shirabeBind($entityId);
        self::$internTable[$entityId] = \WeakReference::create($stub);
        return $stub;
    }

    /**
     * Interns a stub the registry did not build: a `__clone` forwarder rebinds the copy PHP
     * made to a freshly cloned entity, and that pairing has to be visible to later crossings
     * of the same handle.
     */
    public static function adopt(int $entityId, object $stub): void
    {
        self::$internTable[$entityId] = \WeakReference::create($stub);
    }

    /** Invoked from stub destructors. */
    public static function release(int $entityId): void
    {
        unset(self::$internTable[$entityId]);
        RpcRuntime::notifyReleaseHandle($entityId);
    }
}
