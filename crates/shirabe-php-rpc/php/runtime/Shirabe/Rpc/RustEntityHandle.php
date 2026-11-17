<?php

namespace Shirabe\Rpc;

/**
 * The handle of an entity in the Rust side's Rust entity table. Only
 * `Shirabe\RpcRuntime::toRpcValue` builds one, so a record of this class on the wire is always a
 * handle (see docs/dev/php-rpc.md).
 */
final class RustEntityHandle
{
    public int $entityId;
    public string $class;

    public function __construct(int $entityId, string $class)
    {
        $this->entityId = $entityId;
        $this->class = $class;
    }
}
