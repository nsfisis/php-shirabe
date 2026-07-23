<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

use PhpParser\Node;
use PhpParser\Node\Name;
use PhpParser\Node\Stmt;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitor\NameResolver;
use PhpParser\NodeVisitorAbstract;
use PhpParser\Parser;
use PhpParser\ParserFactory;

/**
 * Parses a source tree into ClassInfo records. Names inside declarations
 * and bodies are resolved by PhpParser's NameResolver; docblock types are
 * resolved separately against the recorded per-file use map.
 */
final class SourceParser
{
    private Parser $parser;

    private DocblockTypeExtractor $docblocks;

    /** @var array<string, ClassInfo> FQCN (case-preserved) => info */
    public array $classes = [];

    /** @var array<string, string> lowercase FQCN => case-preserved FQCN */
    public array $lowercaseIndex = [];

    /** @var list<string> docblock tokens that resolved to nothing known; reported, not fatal */
    public array $unresolvedDocblockTypes = [];

    private BuiltinSignatures $builtins;

    public function __construct()
    {
        $this->parser = (new ParserFactory())->createForNewestSupportedVersion();
        $this->docblocks = new DocblockTypeExtractor();
        $this->builtins = new BuiltinSignatures();
    }

    public function parseTree(string $root): void
    {
        $files = [];
        $it = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($root, \FilesystemIterator::SKIP_DOTS));
        foreach ($it as $file) {
            if (!$file->isFile() || $file->getExtension() !== 'php') {
                continue;
            }
            // PHPStan extensions are dev-only tooling, never shipped at
            // runtime; they reference phpstan types and only add noise.
            if (str_contains($file->getPathname(), '/PHPStan/')) {
                continue;
            }
            $files[] = $file->getPathname();
        }
        sort($files);

        // Pre-scan for @template / @phpstan-type alias names so the
        // docblock type extractor can ignore them tree-wide.
        foreach ($files as $path) {
            $code = file_get_contents($path);
            if ($code !== false) {
                $this->docblocks->collectAliases($code);
            }
        }

        foreach ($files as $path) {
            $this->parseFile($path);
        }
    }

    public function finalize(): void
    {
        $this->mergeTraits();
    }

    private function parseFile(string $path): void
    {
        $code = file_get_contents($path);
        if ($code === false) {
            throw new \RuntimeException("cannot read $path");
        }

        $stmts = $this->parser->parse($code);
        if ($stmts === null) {
            throw new \RuntimeException("cannot parse $path");
        }

        $traverser = new NodeTraverser();
        $traverser->addVisitor(new NameResolver());
        $useCollector = new class extends NodeVisitorAbstract {
            public string $namespace = '';

            /** @var array<string, string> lowercase alias => FQCN */
            public array $useMap = [];

            public function enterNode(Node $node): null
            {
                if ($node instanceof Stmt\Namespace_) {
                    $this->namespace = $node->name?->toString() ?? '';
                } elseif ($node instanceof Stmt\Use_ && $node->type === Stmt\Use_::TYPE_NORMAL) {
                    foreach ($node->uses as $use) {
                        $alias = $use->alias?->toString() ?? $use->name->getLast();
                        $this->useMap[strtolower($alias)] = $use->name->toString();
                    }
                } elseif ($node instanceof Stmt\GroupUse) {
                    foreach ($node->uses as $use) {
                        if ($use->type !== Stmt\Use_::TYPE_NORMAL && $node->type !== Stmt\Use_::TYPE_NORMAL) {
                            continue;
                        }
                        $alias = $use->alias?->toString() ?? $use->name->getLast();
                        $this->useMap[strtolower($alias)] = $node->prefix->toString() . '\\' . $use->name->toString();
                    }
                }

                return null;
            }
        };
        $traverser->addVisitor($useCollector);

        $classCollector = new class extends NodeVisitorAbstract {
            /** @var list<Stmt\ClassLike> */
            public array $classLikes = [];

            public function enterNode(Node $node): null
            {
                if ($node instanceof Stmt\ClassLike && $node->name !== null) {
                    $this->classLikes[] = $node;
                }

                return null;
            }
        };
        $traverser->addVisitor($classCollector);

        $traverser->traverse($stmts);

        foreach ($classCollector->classLikes as $node) {
            $this->collectClass($node, $path, $useCollector->namespace, $useCollector->useMap);
        }
    }

    /** @param array<string, string> $useMap */
    private function collectClass(Stmt\ClassLike $node, string $file, string $namespace, array $useMap): void
    {
        $fqcn = $node->namespacedName?->toString() ?? $node->name->toString();

        $kind = match (true) {
            $node instanceof Stmt\Interface_ => 'interface',
            $node instanceof Stmt\Trait_ => 'trait',
            $node instanceof Stmt\Enum_ => 'enum',
            default => 'class',
        };

        $parent = null;
        $interfaces = [];
        if ($node instanceof Stmt\Class_) {
            $parent = $node->extends?->toString();
            foreach ($node->implements as $iface) {
                $interfaces[] = $iface->toString();
            }
        } elseif ($node instanceof Stmt\Interface_) {
            foreach ($node->extends as $iface) {
                $interfaces[] = $iface->toString();
            }
        } elseif ($node instanceof Stmt\Enum_) {
            foreach ($node->implements as $iface) {
                $interfaces[] = $iface->toString();
            }
        }

        $info = new ClassInfo(
            $fqcn,
            $kind,
            $node instanceof Stmt\Class_ && $node->isAbstract(),
            $node instanceof Stmt\Class_ && $node->isFinal(),
            $parent,
            $interfaces,
            $file,
        );

        foreach ($node->getTraitUses() as $traitUse) {
            foreach ($traitUse->traits as $trait) {
                $info->traitUses[] = $trait->toString();
            }
        }

        foreach ($node->getProperties() as $propNode) {
            $doc = $this->docblocks->extract($propNode->getDocComment()?->getText(), $namespace, $useMap);
            [$classTypes, $expandable] = $this->typeClassNames($propNode->type, $fqcn, $parent);
            foreach ($propNode->props as $prop) {
                $info->properties[] = new PropertyInfo(
                    $prop->name->toString(),
                    $propNode->isPrivate() ? 'private' : ($propNode->isProtected() ? 'protected' : 'public'),
                    $propNode->isStatic(),
                    $classTypes,
                    $expandable,
                    $doc['var'],
                );
            }
        }

        foreach ($node->getMethods() as $methodNode) {
            $this->collectMethod($info, $methodNode, $namespace, $useMap);
        }

        $this->classes[$fqcn] = $info;
        $this->lowercaseIndex[strtolower($fqcn)] = $fqcn;
    }

    /** @param array<string, string> $useMap */
    private function collectMethod(ClassInfo $info, Stmt\ClassMethod $node, string $namespace, array $useMap): void
    {
        $doc = $this->docblocks->extract($node->getDocComment()?->getText(), $namespace, $useMap);

        $params = [];
        foreach ($node->params as $paramNode) {
            $name = $paramNode->var instanceof Node\Expr\Variable && is_string($paramNode->var->name)
                ? $paramNode->var->name
                : '';
            [$classTypes, $expandable] = $this->typeClassNames($paramNode->type, $info->fqcn, $info->parent);
            $params[] = new ParamInfo(
                $name,
                $paramNode->byRef,
                $paramNode->variadic,
                $classTypes,
                $expandable,
                $doc['params'][$name] ?? [],
                $this->typeIsCallable($paramNode->type),
            );

            // Constructor property promotion.
            if ($paramNode->flags !== 0) {
                $info->properties[] = new PropertyInfo(
                    $name,
                    ($paramNode->flags & \PhpParser\Modifiers::PRIVATE) !== 0 ? 'private'
                        : (($paramNode->flags & \PhpParser\Modifiers::PROTECTED) !== 0 ? 'protected' : 'public'),
                    false,
                    $classTypes,
                    $expandable,
                    $doc['params'][$name] ?? [],
                );
            }
        }

        [$returnClassTypes, $returnExpandable] = $this->typeClassNames($node->returnType, $info->fqcn, $info->parent);

        $method = new MethodInfo(
            $node->name->toString(),
            $node->isPrivate() ? 'private' : ($node->isProtected() ? 'protected' : 'public'),
            $node->isStatic(),
            $node->isAbstract() || $node->stmts === null,
            $params,
            $returnClassTypes,
            $returnExpandable,
            $doc['return'],
            $doc['throws'],
        );

        $propertyTypes = [];
        foreach ($info->properties as $prop) {
            $single = $this->singleClassType($prop->classTypes, $prop->docblockTypes);
            if ($single !== null) {
                $propertyTypes[$prop->name] = $single;
            }
        }
        $paramTypes = [];
        foreach ($params as $param) {
            $single = $this->singleClassType($param->classTypes, $param->docblockTypes);
            if ($single !== null) {
                $paramTypes[$param->name] = $single;
            }
        }

        $analyzer = BodyAnalyzer::analyze($node, $method, $info->fqcn, $propertyTypes, $paramTypes, $this->builtins);
        foreach ($analyzer->newRefs as $ref) {
            $info->newRefs[] = ltrim($ref, '\\');
        }
        foreach ($analyzer->staticRefs as $ref) {
            $info->staticRefs[] = ltrim($ref, '\\');
        }
        foreach ($analyzer->benignRefs as $ref) {
            $info->benignBodyRefs[] = ltrim($ref, '\\');
        }
        if ($analyzer->writesOwnStaticProps) {
            $info->writesOwnStaticProps = true;
        }

        $info->methods[strtolower($method->name)] = $method;
    }

    /**
     * @param list<string> $native
     * @param list<string> $docblock
     */
    private function singleClassType(array $native, array $docblock): ?string
    {
        if (count($native) === 1) {
            return $native[0];
        }
        if ($native === [] && count($docblock) === 1) {
            return $docblock[0];
        }

        return null;
    }

    /**
     * Native class types that carry a phpstan-only generic payload
     * (`@phpstan-return PromiseInterface<Process>`). The native signature
     * names the wrapper, not the payload, so the payload type is invisible
     * unless the docblock is consulted too — same reason array/iterable/
     * mixed/object trigger docblock refinement below.
     */
    private const GENERIC_WRAPPER_TYPES = [
        'React\\Promise\\PromiseInterface',
    ];

    /**
     * Extracts class-like FQCNs from a native type node and reports whether
     * the type invites docblock refinement (array/iterable/mixed/object,
     * no type at all, or a generic wrapper type from GENERIC_WRAPPER_TYPES).
     *
     * @return array{0: list<string>, 1: bool}
     */
    private function typeClassNames(?Node $type, string $currentClass, ?string $parentClass): array
    {
        if ($type === null) {
            return [[], true];
        }

        $classes = [];
        $expandable = false;

        $walk = function (Node $t) use (&$walk, &$classes, &$expandable, $currentClass, $parentClass): void {
            if ($t instanceof Node\NullableType) {
                $walk($t->type);
            } elseif ($t instanceof Node\UnionType || $t instanceof Node\IntersectionType) {
                foreach ($t->types as $sub) {
                    $walk($sub);
                }
            } elseif ($t instanceof Node\Identifier) {
                if (in_array($t->toLowerString(), ['array', 'iterable', 'mixed', 'object'], true)) {
                    $expandable = true;
                }
            } elseif ($t instanceof Name) {
                $name = ltrim($t->toString(), '\\');
                $lower = strtolower($name);
                if ($lower === 'self' || $lower === 'static') {
                    $classes[] = $currentClass;
                } elseif ($lower === 'parent') {
                    if ($parentClass !== null) {
                        $classes[] = $parentClass;
                    }
                } else {
                    $classes[] = $name;
                    if (in_array($name, self::GENERIC_WRAPPER_TYPES, true)) {
                        $expandable = true;
                    }
                }
            }
        };
        $walk($type);

        return [array_values(array_unique($classes)), $expandable];
    }

    private function typeIsCallable(?Node $type): bool
    {
        if ($type === null) {
            return false;
        }
        if ($type instanceof Node\NullableType) {
            return $this->typeIsCallable($type->type);
        }
        if ($type instanceof Node\UnionType || $type instanceof Node\IntersectionType) {
            foreach ($type->types as $sub) {
                if ($this->typeIsCallable($sub)) {
                    return true;
                }
            }

            return false;
        }
        if ($type instanceof Node\Identifier) {
            return $type->toLowerString() === 'callable';
        }
        if ($type instanceof Name) {
            return strtolower($type->toString()) === 'closure';
        }

        return false;
    }

    private function mergeTraits(): void
    {
        foreach ($this->classes as $info) {
            foreach ($info->traitUses as $traitName) {
                $trait = $this->classes[$traitName] ?? null;
                if ($trait === null) {
                    continue;
                }
                foreach ($trait->methods as $lname => $method) {
                    if (!isset($info->methods[$lname])) {
                        $info->methods[$lname] = $method;
                    }
                }
                foreach ($trait->properties as $prop) {
                    $info->properties[] = $prop;
                }
                foreach ($trait->newRefs as $ref) {
                    $info->newRefs[] = $ref;
                }
                foreach ($trait->staticRefs as $ref) {
                    $info->staticRefs[] = $ref;
                }
                foreach ($trait->benignBodyRefs as $ref) {
                    $info->benignBodyRefs[] = $ref;
                }
                if ($trait->writesOwnStaticProps) {
                    $info->writesOwnStaticProps = true;
                }
            }
        }
    }
}
