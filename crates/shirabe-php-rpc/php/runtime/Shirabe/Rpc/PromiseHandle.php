<?php

namespace Shirabe\Rpc;

/**
 * The handle of a promise (see docs/dev/plugin-async.md). Only `Shirabe\RpcRuntime::toRpcValue`
 * builds one, so a record of this class in a frame payload is always a handle (see
 * docs/dev/php-rpc.md).
 */
final class PromiseHandle
{
    public int $id;

    public function __construct(int $id)
    {
        $this->id = $id;
    }
}
