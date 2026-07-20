<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

use PhpParser\Node;
use PhpParser\Node\Expr;
use PhpParser\Node\Name;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitorAbstract;

/**
 * Walks one method body and records everything the classifier needs from
 * it: direct $this mutations, by-ref builtin usage, self-calls (for purity
 * propagation), statically resolvable external calls passing $this-rooted
 * arguments, referenced types, and static property writes.
 */
final class BodyAnalyzer extends NodeVisitorAbstract
{
    /** @var list<string> */
    public array $newRefs = [];

    /** @var list<string> */
    public array $staticRefs = [];

    /** @var list<string> */
    public array $benignRefs = [];

    public bool $writesOwnStaticProps = false;

    public function __construct(
        private readonly MethodInfo $method,
        private readonly string $currentClass,
        /** @var array<string, string> property name => single class type FQCN */
        private readonly array $propertyTypes,
        /** @var array<string, string> param name => single class type FQCN */
        private readonly array $paramTypes,
        private readonly BuiltinSignatures $builtins,
    ) {
    }

    public static function analyze(ClassMethod $node, MethodInfo $method, string $currentClass, array $propertyTypes, array $paramTypes, BuiltinSignatures $builtins): self
    {
        $analyzer = new self($method, $currentClass, $propertyTypes, $paramTypes, $builtins);
        if ($node->stmts !== null) {
            $traverser = new NodeTraverser();
            $traverser->addVisitor($analyzer);
            $traverser->traverse($node->stmts);
        }

        return $analyzer;
    }

    public function enterNode(Node $node): null
    {
        if ($node instanceof Expr\Assign || $node instanceof Expr\AssignOp || $node instanceof Expr\AssignRef) {
            $this->handleAssignTarget($node->var);
        } elseif ($node instanceof Node\Stmt\Unset_) {
            foreach ($node->vars as $var) {
                if ($this->isThisRooted($var)) {
                    $this->method->mutatesThisDirectly = true;
                }
            }
        } elseif ($node instanceof Expr\PreInc || $node instanceof Expr\PostInc
            || $node instanceof Expr\PreDec || $node instanceof Expr\PostDec) {
            if ($this->isThisRooted($node->var)) {
                $this->method->mutatesThisDirectly = true;
            }
        } elseif ($node instanceof Expr\FuncCall) {
            $this->handleFuncCall($node);
        } elseif ($node instanceof Expr\MethodCall || $node instanceof Expr\NullsafeMethodCall) {
            $this->handleMethodCall($node);
        } elseif ($node instanceof Expr\StaticCall) {
            $this->handleStaticCall($node);
        } elseif ($node instanceof Expr\New_) {
            $this->handleNew($node);
        } elseif ($node instanceof Expr\Instanceof_) {
            if ($node->class instanceof Name) {
                $this->benignRefs[] = $node->class->toString();
            }
        } elseif ($node instanceof Node\Stmt\Catch_) {
            foreach ($node->types as $type) {
                $this->benignRefs[] = $type->toString();
            }
        } elseif ($node instanceof Expr\ClassConstFetch) {
            if ($node->class instanceof Name && !$this->isSelfLike($node->class)) {
                $this->benignRefs[] = $node->class->toString();
            }
        } elseif ($node instanceof Expr\StaticPropertyFetch) {
            // Reads are benign; writes are caught via handleAssignTarget.
            if ($node->class instanceof Name && !$this->isSelfLike($node->class)) {
                $this->benignRefs[] = $node->class->toString();
            }
        }

        return null;
    }

    private function handleAssignTarget(Expr $target): void
    {
        // Destructuring assigns to several targets at once.
        if ($target instanceof Expr\List_ || $target instanceof Expr\Array_) {
            foreach ($target->items as $item) {
                if ($item !== null) {
                    $this->handleAssignTarget($item->value);
                }
            }

            return;
        }

        if ($this->isThisRooted($target)) {
            $this->method->mutatesThisDirectly = true;

            return;
        }

        $root = $this->rootOf($target);
        if ($root instanceof Expr\StaticPropertyFetch && $root->class instanceof Name) {
            if ($this->isSelfLike($root->class) || $root->class->toString() === $this->currentClass) {
                $this->writesOwnStaticProps = true;
            } else {
                $this->staticRefs[] = $root->class->toString();
            }
        }
    }

    private function handleFuncCall(Expr\FuncCall $call): void
    {
        $thisArgs = $this->thisArgPositions($call->args);

        if (!$call->name instanceof Name) {
            if ($thisArgs !== []) {
                $this->method->thisEscapesUnresolved = true;
            }

            return;
        }

        if ($thisArgs === []) {
            return;
        }

        $name = strtolower($call->name->toString());

        // Callback-forwarding builtins re-dispatch their arguments to an
        // unknown callee, so the stub signature (no by-ref) is not the
        // whole truth: [$this, 'method'] can reach a mutator.
        if (in_array($name, ['call_user_func', 'call_user_func_array', 'array_walk', 'array_walk_recursive', 'usort', 'uasort', 'uksort'], true)) {
            $this->method->thisEscapesUnresolved = true;

            return;
        }

        $info = $this->builtins->byRefInfo($name);
        if ($info === null) {
            // Not a known builtin (user-land global function, unknown
            // extension): conservative.
            $this->method->thisEscapesUnresolved = true;

            return;
        }

        foreach ($thisArgs as $pos) {
            if (in_array($pos, $info['fixed'], true)
                || ($info['variadicFrom'] !== null && $pos >= $info['variadicFrom'])) {
                $this->method->mutatesThisDirectly = true;

                return;
            }
        }
    }

    private function handleMethodCall(Expr\MethodCall|Expr\NullsafeMethodCall $call): void
    {
        $thisArgs = $this->thisArgPositions($call->args);
        $literalName = $call->name instanceof Node\Identifier ? strtolower($call->name->toString()) : null;

        $receiverIsThis = $call->var instanceof Expr\Variable && $call->var->name === 'this';

        if ($receiverIsThis) {
            if ($literalName === null) {
                // $this->$method(): both the callee and any argument escape
                // are unresolvable.
                $this->method->thisEscapesUnresolved = true;

                return;
            }
            $this->method->selfCalls[] = ['name' => $literalName, 'thisArgs' => $thisArgs];

            return;
        }

        if ($thisArgs === []) {
            return;
        }

        $receiverClass = $this->resolveReceiverClass($call->var);
        if ($receiverClass === null || $literalName === null) {
            $this->method->thisEscapesUnresolved = true;

            return;
        }

        $this->method->externalCalls[] = ['class' => $receiverClass, 'method' => $literalName, 'thisArgs' => $thisArgs];
    }

    private function handleStaticCall(Expr\StaticCall $call): void
    {
        $thisArgs = $this->thisArgPositions($call->args);
        $literalName = $call->name instanceof Node\Identifier ? strtolower($call->name->toString()) : null;

        if ($call->class instanceof Name && $this->isSelfLike($call->class)) {
            if ($literalName === null) {
                $this->method->thisEscapesUnresolved = true;

                return;
            }
            $this->method->selfCalls[] = ['name' => $literalName, 'thisArgs' => $thisArgs];

            return;
        }

        if ($call->class instanceof Name) {
            $className = $call->class->toString();
            $this->staticRefs[] = $className;
            if ($thisArgs !== []) {
                if ($literalName === null) {
                    $this->method->thisEscapesUnresolved = true;
                } else {
                    $this->method->externalCalls[] = ['class' => $className, 'method' => $literalName, 'thisArgs' => $thisArgs];
                }
            }

            return;
        }

        if ($thisArgs !== []) {
            $this->method->thisEscapesUnresolved = true;
        }
    }

    private function handleNew(Expr\New_ $new): void
    {
        $thisArgs = $this->thisArgPositions($new->args);

        if (!$new->class instanceof Name) {
            if ($thisArgs !== []) {
                $this->method->thisEscapesUnresolved = true;
            }

            return;
        }

        if ($this->isSelfLike($new->class)) {
            return;
        }

        $className = $new->class->toString();
        $this->newRefs[] = $className;
        if ($thisArgs !== []) {
            $this->method->externalCalls[] = ['class' => $className, 'method' => '__construct', 'thisArgs' => $thisArgs];
        }
    }

    /** @param array<Node\Arg|Node\VariadicPlaceholder> $args */
    private function thisArgPositions(array $args): array
    {
        $positions = [];
        foreach ($args as $i => $arg) {
            if (!$arg instanceof Node\Arg) {
                continue;
            }
            if ($this->isThisRooted($arg->value)) {
                $positions[] = $i;
                continue;
            }
            // [$this, 'method'] callback literals: the receiver escapes
            // into the argument even though the array itself is fresh.
            if ($arg->value instanceof Expr\Array_) {
                foreach ($arg->value->items as $item) {
                    if ($item !== null && $this->isThisRooted($item->value)) {
                        $positions[] = $i;
                        break;
                    }
                }
            }
        }

        return $positions;
    }

    private function isThisRooted(Expr $expr): bool
    {
        $root = $this->rootOf($expr);

        return $root instanceof Expr\Variable && $root->name === 'this';
    }

    private function rootOf(Expr $expr): Expr
    {
        while (true) {
            if ($expr instanceof Expr\PropertyFetch || $expr instanceof Expr\NullsafePropertyFetch) {
                $expr = $expr->var;
            } elseif ($expr instanceof Expr\ArrayDimFetch) {
                $expr = $expr->var;
            } else {
                return $expr;
            }
        }
    }

    private function isSelfLike(Name $name): bool
    {
        return in_array(strtolower($name->toString()), ['self', 'static', 'parent'], true);
    }

    /** Resolve the receiver's class when it is a typed property or param. */
    private function resolveReceiverClass(Expr $receiver): ?string
    {
        if (($receiver instanceof Expr\PropertyFetch || $receiver instanceof Expr\NullsafePropertyFetch)
            && $receiver->var instanceof Expr\Variable && $receiver->var->name === 'this'
            && $receiver->name instanceof Node\Identifier) {
            return $this->propertyTypes[$receiver->name->toString()] ?? null;
        }

        if ($receiver instanceof Expr\Variable && is_string($receiver->name)) {
            return $this->paramTypes[$receiver->name] ?? null;
        }

        return null;
    }
}
