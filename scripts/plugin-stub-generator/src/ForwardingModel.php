<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Node;
use PhpParser\Node\Expr\PropertyFetch;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Expr\Variable;
use PhpParser\Node\Name;
use PhpParser\Node\Scalar\String_;
use PhpParser\Node\Stmt\Class_;
use PhpParser\NodeFinder;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitor\NameResolver;
use PhpParser\ParserFactory;

/**
 * The classes `ForwardingModules` emits a module for, and what each of them forwards.
 *
 * The Composer classes come from the classifier report: the stub and runtime classes forward the
 * members it lists for them, and the guard classes the static methods it lists. The worker
 * runtime's own classes (`Shirabe\RustPluginStub` and the like, under php/runtime/Shirabe/) stand
 * for Rust-side objects with no Composer class of their own, so the classifier knows nothing
 * about them; what they forward is read from their `RpcRuntime::callRust` calls, and the Rust
 * side answers every one of those members.
 */
final class ForwardingModel
{
    /** What the Rust entity table answers for every entity, whatever class it crosses as. */
    private const GENERIC_METHODS = ['__get', '__set', '__isset', '__unset', '__shirabe_clone'];

    /**
     * @param list<string> $local `FQCN::method` entries a stub answers in the worker instead of
     *                            forwarding
     * @return array<string, array{guard: bool, parent: ?string, abstract: bool, methods: list<array{name: string, status: string, reason?: string}>, statics: list<array{name: string, status: string, reason?: string}>, constructor: ?array{status: string, reason?: string}}>
     */
    public static function build(Report $report, string $runtimeShirabeDir, array $local): array
    {
        $classes = [];
        foreach ($report->forwarding() as $fqcn) {
            $guard = $report->treatment($fqcn) === 'guard';
            $entry = [
                'guard' => $guard,
                'parent' => null,
                'abstract' => $report->isAbstract($fqcn),
                'methods' => [],
                'statics' => [],
                'constructor' => null,
            ];
            foreach ($report->forwarded($fqcn) as $member) {
                $row = ['name' => $member['member'], 'status' => $member['status'] === 'forward' ? 'implemented' : $member['status']];
                if (isset($member['reason']) && $member['status'] === 'refused') {
                    $row['reason'] = $member['reason'];
                }
                if ($member['kind'] === 'method') {
                    if (in_array("$fqcn::{$member['member']}", $local, true)) {
                        continue;
                    }
                    $entry['methods'][] = $row;
                } elseif ($member['kind'] === 'static') {
                    $entry['statics'][] = $row;
                } else {
                    unset($row['name']);
                    $entry['constructor'] = $row;
                }
            }
            if ($guard && $entry['statics'] === []) {
                continue;
            }
            $classes[$fqcn] = $entry;
        }
        foreach ($classes as $fqcn => &$entry) {
            $parent = $report->parent($fqcn);
            if (!$entry['guard'] && $parent !== null && isset($classes[$parent]) && !$classes[$parent]['guard']) {
                $entry['parent'] = $parent;
            }
        }
        unset($entry);

        foreach (self::runtimeClasses($runtimeShirabeDir) as $fqcn => $entry) {
            $classes[$fqcn] = $entry;
        }
        foreach ($classes as $fqcn => &$entry) {
            if ($entry['parent'] === null && isset($entry['runtimeParent'])) {
                $parent = $entry['runtimeParent'];
                if (isset($classes[$parent])) {
                    $entry['parent'] = $parent;
                }
            }
            unset($entry['runtimeParent']);
        }
        unset($entry);

        return $classes;
    }

    /** @return array<string, array<string, mixed>> */
    private static function runtimeClasses(string $dir): array
    {
        $parser = (new ParserFactory())->createForNewestSupportedVersion();
        $finder = new NodeFinder();
        $paths = [];
        $iterator = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($dir, \FilesystemIterator::SKIP_DOTS));
        foreach ($iterator as $entry) {
            if ($entry->isFile() && str_ends_with($entry->getFilename(), '.php')) {
                $paths[] = $entry->getPathname();
            }
        }
        sort($paths);
        $classes = [];
        foreach ($paths as $path) {
            $traverser = new NodeTraverser();
            $traverser->addVisitor(new NameResolver());
            $ast = $traverser->traverse($parser->parse((string) file_get_contents($path)));
            foreach ($finder->findInstanceOf($ast, Class_::class) as $class) {
                if ($class->namespacedName === null) {
                    continue;
                }
                $fqcn = $class->namespacedName->toString();
                $methods = [];
                foreach ($class->getMethods() as $method) {
                    $where = "$path: $fqcn::{$method->name}";
                    foreach ($finder->find($method->stmts ?? [], self::isRpcRuntimeCall(...)) as $call) {
                        $name = $call->name->toString();
                        if ($name !== 'callRust') {
                            if ($name === 'newRust' || $name === 'callRustStatic') {
                                throw new GenerationError(["$where: a runtime class may forward only instance methods"]);
                            }
                            continue;
                        }
                        $handle = $call->args[0]->value ?? null;
                        $member = $call->args[1]->value ?? null;
                        if (!$member instanceof String_) {
                            throw new GenerationError(["$where calls callRust without a literal method name"]);
                        }
                        if (!($handle instanceof PropertyFetch
                            && $handle->var instanceof Variable && $handle->var->name === 'this'
                            && $handle->name instanceof Node\Identifier && $handle->name->toString() === '__entityId')) {
                            throw new GenerationError(["$where calls callRust on a handle other than its own"]);
                        }
                        if (!in_array($member->value, self::GENERIC_METHODS, true)) {
                            $methods[$member->value] = ['name' => $member->value, 'status' => 'implemented'];
                        }
                    }
                }
                if ($methods === []) {
                    continue;
                }
                $classes[$fqcn] = [
                    'guard' => false,
                    'parent' => null,
                    'runtimeParent' => $class->extends?->toString(),
                    'abstract' => $class->isAbstract(),
                    'methods' => array_values($methods),
                    'statics' => [],
                    'constructor' => null,
                ];
            }
        }

        return $classes;
    }

    private static function isRpcRuntimeCall(Node $node): bool
    {
        return $node instanceof StaticCall
            && $node->class instanceof Name
            && $node->class->toString() === 'Shirabe\RpcRuntime'
            && $node->name instanceof Node\Identifier;
    }
}
