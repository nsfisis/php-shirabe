<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node\Name;
use PhpParser\Node\Stmt\ClassLike;
use PhpParser\Node\Stmt\GroupUse;
use PhpParser\Node\Stmt\Namespace_;
use PhpParser\Node\Stmt\Use_;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitor\NameResolver;
use PhpParser\Parser;

/**
 * A parsed source file: the class-like it declares, its namespace and its import table.
 * All Name nodes in the AST carry a `resolvedName` attribute (NameResolver with
 * replaceNodes disabled), so signatures can be re-rendered under another file's context.
 */
final class SourceFile
{
    public ?string $namespace = null;

    /** @var array<string, string> alias as written => FQCN, in declaration order */
    public array $aliases = [];

    public ClassLike $classLike;

    /** @var list<string> */
    public array $lines;

    public function __construct(
        public readonly string $path,
        public readonly string $expectedFqcn,
        Parser $parser,
    ) {
        $code = file_get_contents($path);
        if ($code === false) {
            throw new GenerationError(["cannot read $path"]);
        }
        $this->lines = explode("\n", $code);

        $ast = $parser->parse($code);
        if ($ast === null) {
            throw new GenerationError(["cannot parse $path"]);
        }
        $traverser = new NodeTraverser();
        $traverser->addVisitor(new NameResolver(null, ['replaceNodes' => false]));
        $ast = $traverser->traverse($ast);

        $classLike = null;
        foreach ($ast as $stmt) {
            if (!$stmt instanceof Namespace_) {
                continue;
            }
            $this->namespace = $stmt->name?->toString();
            foreach ($stmt->stmts as $inner) {
                if ($inner instanceof Use_) {
                    if ($inner->type !== Use_::TYPE_NORMAL) {
                        throw new GenerationError(["$path: function/const use statements are not supported"]);
                    }
                    foreach ($inner->uses as $use) {
                        $this->aliases[$use->getAlias()->toString()] = $use->name->toString();
                    }
                } elseif ($inner instanceof GroupUse) {
                    throw new GenerationError(["$path: group use statements are not supported"]);
                } elseif ($inner instanceof ClassLike) {
                    $fqcn = ($this->namespace === null ? '' : $this->namespace . '\\') . $inner->name?->toString();
                    if ($fqcn === $expectedFqcn) {
                        $classLike = $inner;
                    }
                }
            }
        }
        if ($classLike === null) {
            throw new GenerationError(["$path does not declare $expectedFqcn"]);
        }
        $this->classLike = $classLike;
    }

    /** The declaration's verbatim source text, without any leading doc comment. */
    public function verbatim(int $startLine, int $endLine): string
    {
        return implode("\n", array_slice($this->lines, $startLine - 1, $endLine - $startLine + 1));
    }

    /** This file's imports, restricted to the names the emitted text actually uses. */
    public function importsUsedBy(string $emittedText): string
    {
        $kept = [];
        foreach ($this->aliases as $alias => $fqcn) {
            if (preg_match('/(?<![\\\\$\w])' . preg_quote($alias, '/') . '\b/', $emittedText) === 1) {
                $kept[] = 'use ' . $fqcn
                    . (str_ends_with($fqcn, '\\' . $alias) || $fqcn === $alias ? '' : " as $alias") . ';';
            }
        }
        return implode("\n", $kept);
    }

    /** The FQCN a Name node resolved to, the NameResolver having left the node itself alone. */
    public static function resolvedName(Name $name): string
    {
        $resolved = $name->getAttribute('resolvedName');
        return $resolved instanceof Name ? $resolved->toString() : $name->toString();
    }
}
