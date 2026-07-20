<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class PropertyInfo
{
    public function __construct(
        public readonly string $name,
        /** 'public' | 'protected' | 'private' */
        public readonly string $visibility,
        public readonly bool $static,
        /** Class-like FQCNs in the native type. @var list<string> */
        public readonly array $classTypes,
        /** Native type is array/iterable/mixed/object or absent. */
        public readonly bool $expandable,
        /** Class-like FQCNs from the `@var` docblock. @var list<string> */
        public readonly array $docblockTypes,
    ) {
    }
}
