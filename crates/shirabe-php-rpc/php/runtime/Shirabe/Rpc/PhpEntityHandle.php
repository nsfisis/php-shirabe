<?php

namespace Shirabe\Rpc;

/**
 * The handle of an entity in the worker's PHP entity table. Only `Shirabe\RpcRuntime::toRpcValue`
 * builds one, so a record of this class in a frame payload is always a handle (see
 * docs/dev/php-rpc.md).
 */
final class PhpEntityHandle
{
    public int $entityId;
    public string $class;
    /** @var list<string> */
    public array $implements;

    /** @param list<string> $implements */
    public function __construct(int $entityId, string $class, array $implements)
    {
        $this->entityId = $entityId;
        $this->class = $class;
        $this->implements = $implements;
    }
}
