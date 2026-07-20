<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Extracts class-like type names from phpdoc @param / @return / @var /
 * @throws tags and resolves them against the file's namespace and use map.
 *
 * This is deliberately not a full phpdoc type grammar. Composer's docblocks
 * are PHPStan-checked, so tokenizing the type expression and keeping the
 * class-like tokens is sufficient. Tokens are considered class-like only
 * when they contain a namespace separator or start with an uppercase letter;
 * this filters array-shape keys and scalar keywords. Resolved names that do
 * not exist in the symbol table or a known vendor namespace are reported by
 * the caller rather than silently dropped.
 */
final class DocblockTypeExtractor
{
    private const KEYWORDS = [
        'int', 'integer', 'float', 'double', 'string', 'bool', 'boolean',
        'true', 'false', 'null', 'void', 'never', 'mixed', 'scalar', 'array',
        'iterable', 'object', 'callable', 'resource', 'self', 'static',
        'parent', 'this', 'list', 'non-empty-list', 'non-empty-array',
        'non-empty-string', 'class-string', 'callable-string',
        'numeric-string', 'lowercase-string', 'literal-string', 'key-of',
        'value-of', 'array-key', 'positive-int', 'negative-int',
        'non-negative-int', 'non-positive-int', 'int-mask-of', 'Closure',
        'Generator', 'Traversable', 'Iterator', 'IteratorAggregate',
        'ArrayAccess', 'Countable', 'Stringable', 'JsonSerializable',
        'Throwable', 'Exception', 'SplFileInfo', 'ArrayObject',
    ];

    /**
     * Names declared by @template / @phpstan-type / @phpstan-import-type
     * anywhere in the tree; they look like class names inside type
     * expressions but are not classes. Filled by a pre-scan.
     *
     * @var array<string, true>
     */
    public array $aliasNames = [];

    public function collectAliases(string $code): void
    {
        if (preg_match_all(
            '/@(?:phpstan-|psalm-)?(?:template(?:-covariant|-contravariant)?|type|import-type)\s+([A-Za-z_][A-Za-z0-9_]*)/',
            $code,
            $m,
        ) > 0) {
            foreach ($m[1] as $name) {
                $this->aliasNames[$name] = true;
            }
        }
    }

    /**
     * @param array<string, string> $useMap lowercase alias => FQCN
     * @return array{params: array<string, list<string>>, return: list<string>, var: list<string>, throws: list<string>}
     */
    public function extract(?string $docblock, string $namespace, array $useMap): array
    {
        $result = ['params' => [], 'return' => [], 'var' => [], 'throws' => []];
        if ($docblock === null) {
            return $result;
        }

        $pattern = '/@(param|return|var|throws|phpstan-param|phpstan-return|phpstan-var)[ \t]+(.+)$/m';
        if (preg_match_all($pattern, $docblock, $matches, PREG_SET_ORDER) === false) {
            return $result;
        }

        foreach ($matches as $m) {
            $tag = str_replace('phpstan-', '', $m[1]);
            $rest = rtrim($m[2]);
            // The type expression ends at the first whitespace at bracket
            // depth zero; spaces inside array{...} / array<...> shapes are
            // part of the type. Whatever follows is the variable name
            // (for @param) and/or a free-text description.
            $typeExpr = $this->cutAtToplevelSpace($rest);
            $after = substr($rest, strlen($typeExpr));
            $paramName = null;
            if (preg_match('/^\s*\$(\w+)/', $after, $nm) === 1) {
                $paramName = $nm[1];
            }

            $types = $this->classLikeTokens($typeExpr, $namespace, $useMap);
            if ($types === []) {
                continue;
            }

            switch ($tag) {
                case 'param':
                    if ($paramName !== null) {
                        $result['params'][$paramName] = array_values(array_unique(array_merge(
                            $result['params'][$paramName] ?? [],
                            $types,
                        )));
                    }
                    break;
                case 'return':
                    $result['return'] = array_values(array_unique(array_merge($result['return'], $types)));
                    break;
                case 'var':
                    $result['var'] = array_values(array_unique(array_merge($result['var'], $types)));
                    break;
                case 'throws':
                    $result['throws'] = array_values(array_unique(array_merge($result['throws'], $types)));
                    break;
            }
        }

        return $result;
    }

    private function cutAtToplevelSpace(string $expr): string
    {
        $depth = 0;
        $len = strlen($expr);
        for ($i = 0; $i < $len; $i++) {
            $c = $expr[$i];
            if ($c === '<' || $c === '{' || $c === '(' || $c === '[') {
                $depth++;
            } elseif ($c === '>' || $c === '}' || $c === ')' || $c === ']') {
                $depth--;
            } elseif (($c === ' ' || $c === "\t") && $depth === 0) {
                return substr($expr, 0, $i);
            }
        }

        return $expr;
    }

    /**
     * @param array<string, string> $useMap
     * @return list<string>
     */
    private function classLikeTokens(string $typeExpr, string $namespace, array $useMap): array
    {
        // Constant references (self::STABILITY_*, BasePackage::STABILITIES,
        // PATHINFO_EXTENSION|...) are not class names: drop everything after
        // `::`, then drop all-caps tokens.
        $typeExpr = preg_replace('/::[A-Za-z0-9_*]*/', '', $typeExpr) ?? $typeExpr;

        if (preg_match_all('/\\\\?[A-Za-z_][A-Za-z0-9_]*(?:\\\\[A-Za-z_][A-Za-z0-9_]*)*/', $typeExpr, $m) === false) {
            return [];
        }

        $out = [];
        foreach ($m[0] as $token) {
            $isQualified = str_contains($token, '\\');
            if (!$isQualified) {
                if (in_array($token, self::KEYWORDS, true) || in_array(strtolower($token), self::KEYWORDS, true)) {
                    continue;
                }
                // Array-shape keys and phpdoc keywords are lowercase;
                // Composer class names are StudlyCaps.
                if (!ctype_upper($token[0])) {
                    continue;
                }
                // All-caps tokens are constants, not classes.
                if (preg_match('/[a-z]/', $token) !== 1) {
                    continue;
                }
                // Generic parameters and phpstan type aliases.
                if (isset($this->aliasNames[$token])) {
                    continue;
                }
            }
            $out[] = $this->resolve($token, $namespace, $useMap);
        }

        return array_values(array_unique($out));
    }

    /** @param array<string, string> $useMap */
    private function resolve(string $name, string $namespace, array $useMap): string
    {
        if (str_starts_with($name, '\\')) {
            return ltrim($name, '\\');
        }

        $parts = explode('\\', $name);
        $firstLower = strtolower($parts[0]);
        if (isset($useMap[$firstLower])) {
            $parts[0] = $useMap[$firstLower];

            return implode('\\', $parts);
        }

        if ($namespace === '') {
            return $name;
        }

        return $namespace . '\\' . $name;
    }
}
