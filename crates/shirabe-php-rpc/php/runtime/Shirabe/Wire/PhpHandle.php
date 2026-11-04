<?php

namespace Shirabe\Wire;

/**
 * The descriptor of an entity in the worker's P table. Only the wire codec builds one, so a
 * record of this class on the wire is always a descriptor (see docs/dev/php-rpc.md).
 */
final class PhpHandle
{
    public int $phandle;
    public string $class;
    /** @var list<string> */
    public array $implements;

    /** @param list<string> $implements */
    public function __construct(int $phandle, string $class, array $implements)
    {
        $this->phandle = $phandle;
        $this->class = $class;
        $this->implements = $implements;
    }
}
