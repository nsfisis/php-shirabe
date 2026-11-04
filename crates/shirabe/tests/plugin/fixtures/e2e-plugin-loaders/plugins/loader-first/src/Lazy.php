<?php

namespace ShirabeTest\LoaderFirst;

/**
 * Loaded only once the second plugin has been registered.
 */
class Lazy
{
    public function describe(): string
    {
        return 'lazy class loaded';
    }
}
