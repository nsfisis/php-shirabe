<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node;
use PhpParser\Node\Identifier;
use PhpParser\Node\IntersectionType;
use PhpParser\Node\Name;
use PhpParser\Node\NullableType;
use PhpParser\Node\UnionType;
use PhpParser\PrettyPrinter\Standard;

/**
 * Renders type and default-value nodes into a target file context: class names resolve to
 * their FQCN and are re-spelled through the target's import table (alias if imported, bare
 * name if in the stub's namespace, `\FQCN` otherwise). This lets signatures declared in one
 * file (an interface) be emitted into a stub that carries another file's `use` block.
 */
final class NamePrinter extends Standard
{
    private SourceFile $context;

    public function __construct()
    {
        parent::__construct(['shortArraySyntax' => true]);
    }

    public function renderType(?Node $type, SourceFile $context): string
    {
        if ($type === null) {
            return '';
        }
        if ($type instanceof Identifier) {
            return $type->toString();
        }
        if ($type instanceof NullableType) {
            return '?' . $this->renderType($type->type, $context);
        }
        if ($type instanceof UnionType) {
            return implode('|', array_map(fn (Node $t): string => $this->renderType($t, $context), $type->types));
        }
        if ($type instanceof IntersectionType) {
            return implode('&', array_map(fn (Node $t): string => $this->renderType($t, $context), $type->types));
        }
        if ($type instanceof Name) {
            return $this->renderName($type, $context);
        }
        throw new GenerationError(['unsupported type node ' . get_class($type)]);
    }

    public function renderName(Name $name, SourceFile $context): string
    {
        if ($name->isSpecialClassName()) {
            return $name->toString();
        }
        $resolved = $name->getAttribute('resolvedName');
        $fqcn = $resolved instanceof Name ? $resolved->toString() : $name->toString();
        if ($name->isUnqualified() && !str_contains($fqcn, '\\')) {
            // Names that resolved into the global namespace only occur in constant space
            // (true, false, null, ...); they keep their source spelling.
            return $name->toString();
        }
        return $this->renderFqcn($fqcn, $context);
    }

    public function renderFqcn(string $fqcn, SourceFile $context): string
    {
        foreach ($context->aliases as $alias => $target) {
            if ($target === $fqcn) {
                return $alias;
            }
        }
        $prefix = $context->namespace === null ? '' : $context->namespace . '\\';
        if ($prefix !== '' && str_starts_with($fqcn, $prefix) && !str_contains(substr($fqcn, strlen($prefix)), '\\')) {
            return substr($fqcn, strlen($prefix));
        }
        return '\\' . $fqcn;
    }

    public function renderExpr(Node\Expr $expr, SourceFile $context): string
    {
        $this->context = $context;
        return $this->prettyPrintExpr($expr);
    }

    protected function pName(Name $node): string
    {
        return $this->renderName($node, $this->context);
    }
}
