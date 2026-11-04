<?php

namespace Shirabe\Wire;

/**
 * The descriptor of an entity in the Rust side's R table. Only the wire codec builds one, so a
 * record of this class on the wire is always a descriptor (see docs/dev/php-rpc.md).
 */
final class RustHandle
{
    public int $rhandle;
    public string $class;

    public function __construct(int $rhandle, string $class)
    {
        $this->rhandle = $rhandle;
        $this->class = $class;
    }
}
