<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/**
 * Reads the hand-written forwarding impls under a Rust source tree: every block of the form
 * `impl forwarding::<path>::Forwarding for forwarding::<path>::PhpClass { ... }`, and the names of
 * the `fn` items directly inside it, which are the members the impl answers instead of the
 * generated default.
 */
final class RustForwardingImpls
{
    /**
     * @return array<string, list<array{path: string, fns: list<string>}>> module path (`package::locker`)
     *                                                                       => the impls found for it
     */
    public static function in(string $sourceDir): array
    {
        $paths = [];
        $iterator = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($sourceDir, \FilesystemIterator::SKIP_DOTS)
        );
        foreach ($iterator as $entry) {
            if ($entry->isFile() && str_ends_with($entry->getFilename(), '.rs')) {
                $paths[] = $entry->getPathname();
            }
        }
        sort($paths);
        $impls = [];
        foreach ($paths as $path) {
            $code = file_get_contents($path);
            if (!preg_match('/\bForwarding\s+for\s+forwarding::/', $code)) {
                continue;
            }
            foreach (self::inFile($path, RustTokens::tokenize($path, $code)) as [$module, $fns]) {
                $impls[$module][] = ['path' => $path, 'fns' => $fns];
            }
        }

        return $impls;
    }

    /**
     * @param list<array{0: string, 1: string}> $tokens
     * @return list<array{0: string, 1: list<string>}>
     */
    private static function inFile(string $path, array $tokens): array
    {
        $found = [];
        $count = count($tokens);
        for ($i = 0; $i < $count; $i++) {
            if ($tokens[$i] !== ['ident', 'impl']) {
                continue;
            }
            $j = $i + 1;
            $trait = self::path($tokens, $j);
            if ($trait === null || array_pop($trait) !== 'Forwarding' || ($tokens[$j] ?? null) !== ['ident', 'for']) {
                continue;
            }
            $j++;
            $self = self::path($tokens, $j);
            if ($self === null || array_pop($self) !== 'PhpClass' || $self !== $trait) {
                throw new GenerationError(["$path: a forwarding impl whose trait and type name different modules"]);
            }
            if (($tokens[$j] ?? null) !== ['punct', '{']) {
                throw new GenerationError(["$path: a forwarding impl of " . implode('::', $trait) . ' with a where clause or generics']);
            }
            $fns = [];
            $depth = 0;
            for ($k = $j; $k < $count; $k++) {
                $token = $tokens[$k];
                if ($token[0] === 'punct' && in_array($token[1], ['(', '[', '{'], true)) {
                    $depth++;
                } elseif ($token[0] === 'punct' && in_array($token[1], [')', ']', '}'], true)) {
                    $depth--;
                    if ($depth === 0) {
                        break;
                    }
                } elseif ($depth === 1 && $token === ['ident', 'fn'] && ($tokens[$k + 1][0] ?? null) === 'ident') {
                    $name = $tokens[$k + 1][1];
                    $fns[] = str_starts_with($name, 'r#') ? substr($name, 2) : $name;
                }
            }
            $found[] = [implode('::', $trait), $fns];
            $i = $k;
        }

        return $found;
    }

    /**
     * Reads `forwarding::a::b::Name` starting at `$i`, leaving `$i` after it.
     *
     * @param list<array{0: string, 1: string}> $tokens
     * @return list<string>|null the segments after `forwarding`
     */
    private static function path(array $tokens, int &$i): ?array
    {
        if (($tokens[$i] ?? null) !== ['ident', 'forwarding']) {
            return null;
        }
        $segments = [];
        $i++;
        while (($tokens[$i] ?? null) === ['punct', '::'] && ($tokens[$i + 1][0] ?? null) === 'ident') {
            $segments[] = $tokens[$i + 1][1];
            $i += 2;
        }

        return $segments === [] ? null : $segments;
    }
}
