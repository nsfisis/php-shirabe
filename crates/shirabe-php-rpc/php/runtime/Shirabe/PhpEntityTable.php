<?php

namespace Shirabe;

/**
 * The PHP entity table: PHP-owned entities exposed to Rust, keyed by PHP entity id. Entries are
 * strong references — an entity stays alive until the Rust side sends ReleaseHandle for it.
 */
final class PhpEntityTable
{
    /** @var array<int, object> */
    private static array $objects = [];
    /** @var array<int, int> spl_object_id => PHP entity id, so one entity keeps one id */
    private static array $handlesByObjectId = [];
    private static int $nextEntityId = 1;

    public static function register(object $obj): int
    {
        $objectId = spl_object_id($obj);
        $existing = self::$handlesByObjectId[$objectId] ?? null;
        if ($existing !== null && isset(self::$objects[$existing])) {
            return $existing;
        }
        $entityId = self::$nextEntityId++;
        self::$objects[$entityId] = $obj;
        self::$handlesByObjectId[$objectId] = $entityId;
        return $entityId;
    }

    public static function get(int $entityId): object
    {
        if (!isset(self::$objects[$entityId])) {
            throw new \RuntimeException("unknown PHP entity id {$entityId}");
        }
        return self::$objects[$entityId];
    }

    public static function release(int $entityId): void
    {
        $obj = self::$objects[$entityId] ?? null;
        unset(self::$objects[$entityId]);
        if ($obj !== null) {
            unset(self::$handlesByObjectId[spl_object_id($obj)]);
        }
    }

    public static function handle(object $obj): Rpc\PhpEntityHandle
    {
        return new Rpc\PhpEntityHandle(
            self::register($obj),
            get_class($obj),
            array_values(class_implements($obj))
        );
    }
}
