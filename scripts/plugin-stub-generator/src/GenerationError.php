<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

final class GenerationError extends \RuntimeException
{
    /** @param list<string> $errors */
    public function __construct(public readonly array $errors)
    {
        parent::__construct(implode("\n", $errors));
    }
}
