<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class ClassInfo
{
    /** @var array<string, MethodInfo> keyed by lowercase method name */
    public array $methods = [];

    /** @var list<PropertyInfo> */
    public array $properties = [];

    /** @var list<string> FQCNs of used traits */
    public array $traitUses = [];

    /**
     * Types instantiated in method bodies (`new X`). Instantiating a
     * proxied service from real PHP raises the dual-instantiation question,
     * so these are tracked apart from static references.
     *
     * @var list<string>
     */
    public array $newRefs = [];

    /**
     * Types referenced statically in method bodies (`X::method()`, writes
     * to `X::$prop`). Satisfied by any executable presence in the child
     * world, including a generated proxy stub.
     *
     * @var list<string>
     */
    public array $staticRefs = [];

    /**
     * Types referenced from method bodies in ways satisfied by a mere
     * declaration: `instanceof`, `catch`, `X::class`, constant reads.
     *
     * @var list<string>
     */
    public array $benignBodyRefs = [];

    public bool $writesOwnStaticProps = false;

    public function __construct(
        public readonly string $fqcn,
        /** 'class' | 'interface' | 'trait' | 'enum' */
        public readonly string $kind,
        public readonly bool $abstract,
        public readonly bool $final,
        public readonly ?string $parent,
        /** @var list<string> */
        public readonly array $interfaces,
        public readonly string $file,
    ) {
    }

    public function isContractLike(): bool
    {
        return $this->kind === 'interface' || ($this->kind === 'class' && $this->abstract);
    }
}
