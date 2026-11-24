<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class ClassInfo
{
    /** @var array<string, MethodInfo> keyed by lowercase method name */
    public array $methods = [];

    /** @var list<PropertyInfo> */
    public array $properties = [];

    /** @var array<string, true> lowercase names of the methods merged in from used traits */
    public array $traitMethods = [];

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
     * to `X::$prop`). Satisfied by any executable presence on the PHP
     * side, including a generated proxy stub.
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

    /** @var list<string> own static properties written from method bodies; `*` for a dynamic name */
    public array $staticWrites = [];

    /**
     * References that need executable code behind the target, per member:
     * `new X` (target `X`) and `X::m()` / writes to `X::$p` (target
     * `X::m` / `X::$p`).
     *
     * @var list<array{member: string, kind: 'new'|'static', target: string}>
     */
    public array $hardRefs = [];

    public function __construct(
        public readonly string $fqcn,
        /** 'class' | 'interface' | 'trait' | 'enum' */
        public readonly string $kind,
        public readonly bool $abstract,
        public readonly bool $final,
        public readonly ?string $parent,
        /** @var list<string> */
        public readonly array $interfaces,
        public readonly string $file
    ) {}

    public function isContractLike(): bool
    {
        return $this->kind === 'interface' || $this->kind === 'class' && $this->abstract;
    }
}
