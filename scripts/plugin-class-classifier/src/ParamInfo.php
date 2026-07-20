<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class ParamInfo
{
    public function __construct(
        public readonly string $name,
        public readonly bool $byRef,
        public readonly bool $variadic,
        /** Class-like FQCNs in the native type. @var list<string> */
        public readonly array $classTypes,
        /** Native type is array/iterable/mixed/object or absent. */
        public readonly bool $expandable,
        /** Class-like FQCNs from the `@param` docblock. @var list<string> */
        public readonly array $docblockTypes,
        /** Native type mentions callable or \Closure: the value is a
         *  callback that needs a handle across the boundary. */
        public readonly bool $callable = false,
    ) {
    }
}
