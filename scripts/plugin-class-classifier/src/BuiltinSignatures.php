<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

use JetBrains\PHPStormStub\PhpStormStubsMap;
use PhpParser\Node;
use PhpParser\NodeTraverser;
use PhpParser\NodeVisitorAbstract;
use PhpParser\Parser;
use PhpParser\ParserFactory;

/**
 * By-ref parameter positions of PHP builtin functions, resolved from
 * jetbrains/phpstorm-stubs instead of a hand-maintained table. Stub files
 * are parsed lazily, one file per first lookup of any function it defines;
 * a function absent from the stubs map is unknown and the caller must be
 * conservative.
 */
final class BuiltinSignatures
{
    private Parser $parser;

    /** @var array<string, string> lowercase function name => stub file relative path */
    private array $functionFiles = [];

    /** @var array<string, array{fixed: list<int>, variadicFrom: int|null}> lowercase function name => by-ref info */
    private array $byRef = [];

    /** @var array<string, true> */
    private array $parsedFiles = [];

    public function __construct()
    {
        $this->parser = (new ParserFactory())->createForNewestSupportedVersion();
        foreach (PhpStormStubsMap::FUNCTIONS as $name => $file) {
            $this->functionFiles[strtolower((string) $name)] = $file;
        }
    }

    /**
     * @return array{fixed: list<int>, variadicFrom: int|null}|null
     *         null when the function is not a known builtin
     */
    public function byRefInfo(string $lowerName): ?array
    {
        if (isset($this->byRef[$lowerName])) {
            return $this->byRef[$lowerName];
        }

        $file = $this->functionFiles[$lowerName] ?? null;
        if ($file === null) {
            return null;
        }

        $this->parseStubFile($file);

        // Defined in the map but somehow absent from the parsed file:
        // treat as unknown rather than silently non-by-ref.
        return $this->byRef[$lowerName] ?? null;
    }

    private function parseStubFile(string $relativePath): void
    {
        if (isset($this->parsedFiles[$relativePath])) {
            return;
        }
        $this->parsedFiles[$relativePath] = true;

        $path = PhpStormStubsMap::DIR . '/' . $relativePath;
        $code = file_get_contents($path);
        if ($code === false) {
            throw new \RuntimeException("cannot read stub file $path");
        }
        $stmts = $this->parser->parse($code);
        if ($stmts === null) {
            throw new \RuntimeException("cannot parse stub file $path");
        }

        $byRef = &$this->byRef;
        $collector = new class($byRef) extends NodeVisitorAbstract {
            /** @param array<string, array{fixed: list<int>, variadicFrom: int|null}> $byRef */
            public function __construct(private array &$byRef)
            {
            }

            public function enterNode(Node $node): null
            {
                if (!$node instanceof Node\Stmt\Function_) {
                    return null;
                }

                $name = strtolower($node->name->toString());
                $fixed = [];
                $variadicFrom = null;
                foreach ($node->params as $i => $param) {
                    if (!$param->byRef) {
                        continue;
                    }
                    if ($param->variadic) {
                        $variadicFrom = $variadicFrom === null ? $i : min($variadicFrom, $i);
                    } else {
                        $fixed[] = $i;
                    }
                }

                // Stub files occasionally declare a function more than once
                // (per-version signatures); merge conservatively.
                if (isset($this->byRef[$name])) {
                    $fixed = array_values(array_unique(array_merge($this->byRef[$name]['fixed'], $fixed)));
                    sort($fixed);
                    $prev = $this->byRef[$name]['variadicFrom'];
                    $variadicFrom = $prev === null ? $variadicFrom : ($variadicFrom === null ? $prev : min($prev, $variadicFrom));
                }
                $this->byRef[$name] = ['fixed' => $fixed, 'variadicFrom' => $variadicFrom];

                return null;
            }
        };

        $traverser = new NodeTraverser();
        $traverser->addVisitor($collector);
        $traverser->traverse($stmts);
    }
}
