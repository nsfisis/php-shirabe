<?php

namespace Shirabe\Wire;

/**
 * The descriptor of a job (see docs/dev/plugin-async.md). Only the wire codec builds one, so a
 * record of this class on the wire is always a descriptor (see docs/dev/php-rpc.md).
 */
final class Job
{
    public int $id;

    public function __construct(int $id)
    {
        $this->id = $id;
    }
}
