<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

final class MethodInfo
{
    /** Filled by BodyAnalyzer. */
    public bool $mutatesThisDirectly = false;

    /**
     * Calls to methods of the same object ($this->m(), self::m(),
     * static::m(), parent::m()) with a literal name. Purity propagates
     * through these; thisArgs lists 0-based argument positions holding
     * $this-rooted expressions (checked against by-ref parameters).
     *
     * @var list<array{name: string, thisArgs: list<int>}>
     */
    public array $selfCalls = [];

    /**
     * Calls on receivers whose class could be resolved statically (typed
     * property or typed parameter) that pass $this-rooted expressions.
     * Checked against the callee's by-ref parameter positions once the whole
     * symbol table is available.
     *
     * @var list<array{class: string, method: string, thisArgs: list<int>}>
     */
    public array $externalCalls = [];

    /**
     * True when the body contains a call whose signature cannot be resolved
     * statically (dynamic method name, variable function, call_user_func,
     * closure) with a $this-rooted expression (or $this itself) as argument,
     * or passes a $this-rooted expression into a by-ref parameter position.
     */
    public bool $thisEscapesUnresolved = false;

    public function __construct(
        public readonly string $name,
        /** 'public' | 'protected' | 'private' */
        public readonly string $visibility,
        public readonly bool $static,
        public readonly bool $abstract,
        /** @var list<ParamInfo> */
        public readonly array $params,
        /** Class-like FQCNs in the native return type. @var list<string> */
        public readonly array $returnClassTypes,
        /** Native return type is array/iterable/mixed/object or absent. */
        public readonly bool $returnExpandable,
        /** Class-like FQCNs from the `@return` docblock. @var list<string> */
        public readonly array $docblockReturnTypes,
        /** Class-like FQCNs from `@throws` docblocks. @var list<string> */
        public readonly array $docblockThrowsTypes,
    ) {
    }

    /** @return list<int> positions of by-ref parameters */
    public function byRefParamPositions(): array
    {
        $positions = [];
        foreach ($this->params as $i => $param) {
            if ($param->byRef) {
                $positions[] = $i;
            }
        }

        return $positions;
    }
}
