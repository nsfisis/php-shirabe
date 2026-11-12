<?php

namespace Shirabe;

/**
 * The P table: PHP-owned entities exposed to Rust, keyed by phandle. Entries are strong
 * references — an entity stays alive until the Rust side sends ReleasePhpHandle.
 */
final class PhpObjectRegistry
{
    /** @var array<int, object> */
    private static array $objects = [];
    /** @var array<int, int> spl_object_id => phandle, so one entity keeps one handle */
    private static array $handlesByObjectId = [];
    private static int $nextPhandle = 1;

    public static function register(object $obj): int
    {
        $objectId = spl_object_id($obj);
        $existing = self::$handlesByObjectId[$objectId] ?? null;
        if ($existing !== null && isset(self::$objects[$existing])) {
            return $existing;
        }
        $phandle = self::$nextPhandle++;
        self::$objects[$phandle] = $obj;
        self::$handlesByObjectId[$objectId] = $phandle;
        return $phandle;
    }

    public static function get(int $phandle): object
    {
        if (!isset(self::$objects[$phandle])) {
            throw new \RuntimeException("unknown PHP handle {$phandle}");
        }
        return self::$objects[$phandle];
    }

    public static function release(int $phandle): void
    {
        $obj = self::$objects[$phandle] ?? null;
        unset(self::$objects[$phandle]);
        if ($obj !== null) {
            unset(self::$handlesByObjectId[spl_object_id($obj)]);
        }
    }

    public static function descriptor(object $obj): Wire\PhpHandle
    {
        return new Wire\PhpHandle(
            self::register($obj),
            get_class($obj),
            array_values(class_implements($obj))
        );
    }
}
