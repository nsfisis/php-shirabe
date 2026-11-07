<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;
use Shirabe\Lint\Support\UseTree;

/**
 * A `use` path does not spell out a submodule whose items its parent already re-exports with
 * `pub use <child>::*;`: `use crate::exception::ExceptionInterface;`, not
 * `use crate::exception::exception_interface::ExceptionInterface;`. Paths rooted at `crate` and at
 * workspace crates are checked.
 *
 * Only modules that stand for a single PHP file are re-exported that way. A module with its own
 * directory stands for a PHP namespace and keeps its segment, so `pub use <child>::*;` of such a
 * module is reported too.
 *
 * A segment is kept when dropping it could change what the path resolves to: the parent defines or
 * imports the next name itself (an explicit item shadows a glob), or another glob-re-exported
 * sibling also declares it (the shorter path would be ambiguous). Re-exports under `#[cfg(...)]`
 * are ignored, as the shorter path would not resolve on every target.
 */
final class NoRedundantModulePath implements Linter
{
    private const USE_START_RE = '/\A(?:pub(?:\([^)]*\))?\s+)?use\b/';
    private const GLOB_REEXPORT_RE = '/\Apub(?:\(crate\))?\s+use\s+(?:r#)?([a-z_][a-z0-9_]*)::\*;/';
    private const ITEM_RE = '/^(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?(?:struct|enum|trait|fn|type|const|static|mod|union)\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)/m';

    /** @var array<string, string> crate name (`shirabe_php_shim`) => src directory */
    private array $crateSrcDirs = [];

    /** @var array<string, array{globs: list<string>, names: array<string, true>}|null> */
    private array $moduleCache = [];

    /** @var array<string, array<string, true>> */
    private array $itemCache = [];

    public function name(): string
    {
        return 'no_redundant_module_path';
    }

    public function failureIntro(): string
    {
        return "Found `use` paths that spell out a submodule its parent re-exports with `pub use <child>::*;`,\n"
            . 'or glob re-exports of a namespace module (one with its own directory).'
            . ' Import the item through the parent module, and keep namespace segments in paths:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $this->crateSrcDirs = [];
        $this->moduleCache = [];
        $this->itemCache = [];
        foreach (glob("{$rootDir}/crates/*/src", GLOB_ONLYDIR) ?: [] as $srcDir) {
            $this->crateSrcDirs[str_replace('-', '_', basename(dirname($srcDir)))] = $srcDir;
        }

        $errors = [];

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }

            foreach ($this->useStatements($path) as [$line, , $statement, $isTopLevel, $attr]) {
                if ($isTopLevel && !str_contains($attr, '#[cfg') && preg_match(self::GLOB_REEXPORT_RE, $statement, $gm)
                    && is_dir($this->childDir($path, $gm[1]))) {
                    $errors[] = "{$relative}:{$line}: `{$statement}` re-exports the namespace module `{$gm[1]}`";
                }
            }

            $ownCrate = preg_match('#\Acrates/([^/]+)/#', $relative, $m) ? str_replace('-', '_', $m[1]) : null;

            foreach ($this->useStatements($path) as [$line, $tree]) {
                foreach (UseTree::expand($tree) as $usePath) {
                    $plain = trim(preg_replace('/\s+as\s+\S+\s*\z/', '', $usePath));
                    $shortest = $this->shorten($plain, $ownCrate);
                    if ($shortest !== $plain) {
                        $errors[] = "{$relative}:{$line}: `{$plain}` (use `{$shortest}`)";
                    }
                }
            }
        }

        return array_values(array_unique($errors));
    }

    private function shorten(string $path, ?string $ownCrate): string
    {
        $segments = array_map('trim', explode('::', $path));
        $crate = $segments[0] === 'crate' ? $ownCrate : $segments[0];
        if ($crate === null || !isset($this->crateSrcDirs[$crate])) {
            return $path;
        }

        $last = $segments[count($segments) - 1];
        if (count($segments) < 3 || $last === '*' || $last === 'self') {
            return $path;
        }

        // Decide from the deepest module up, so each check sees the name the shortened path
        // exposes at that level. Modules are still resolved through the original segments.
        $kept = [count($segments) - 1 => $last];
        for ($i = count($segments) - 2; $i >= 1; $i--) {
            $exposed = reset($kept);
            if (!$this->isRedundant($crate, array_slice($segments, 1, $i - 1), $segments[$i], $exposed)) {
                $kept = [$i => $segments[$i]] + $kept;
            }
        }

        return implode('::', [$segments[0], ...array_values($kept)]);
    }

    /** @param list<string> $module */
    private function isRedundant(string $crate, array $module, string $child, string $next): bool
    {
        $info = $this->moduleInfo($crate, $module);
        if ($info === null || !in_array($child, $info['globs'], true) || isset($info['names'][$next])) {
            return false;
        }

        foreach ($info['globs'] as $sibling) {
            if ($sibling !== $child && isset($this->items($crate, [...$module, $sibling])[$next])) {
                return false;
            }
        }

        return true;
    }

    /**
     * @param list<string> $module
     * @return array{globs: list<string>, names: array<string, true>}|null
     */
    private function moduleInfo(string $crate, array $module): ?array
    {
        $key = $crate . '::' . implode('::', $module);
        if (array_key_exists($key, $this->moduleCache)) {
            return $this->moduleCache[$key];
        }

        $file = $this->moduleFile($crate, $module);
        if ($file === null) {
            return $this->moduleCache[$key] = null;
        }

        $globs = [];
        $names = [];
        foreach ($this->useStatements($file) as [, $tree, $statement, $isTopLevel, $attr]) {
            if (!$isTopLevel) {
                continue;
            }
            if (preg_match(self::GLOB_REEXPORT_RE, $statement, $m) && !str_contains($attr, '#[cfg')) {
                $globs[] = $m[1];
                continue;
            }
            foreach (UseTree::expand($tree) as $usePath) {
                if (preg_match('/\s+as\s+(\S+)\s*\z/', $usePath, $am)) {
                    $names[$am[1]] = true;
                } elseif (preg_match('/([A-Za-z_][A-Za-z0-9_]*)\s*\z/', $usePath, $nm)) {
                    $names[$nm[1]] = true;
                }
            }
        }
        $names += $this->items($crate, $module);

        return $this->moduleCache[$key] = ['globs' => $globs, 'names' => $names];
    }

    /**
     * Names of the items a module file declares at its top level.
     *
     * @param list<string> $module
     * @return array<string, true>
     */
    private function items(string $crate, array $module): array
    {
        $key = $crate . '::' . implode('::', $module);
        if (isset($this->itemCache[$key])) {
            return $this->itemCache[$key];
        }

        $file = $this->moduleFile($crate, $module);
        $names = [];
        if ($file !== null) {
            preg_match_all(self::ITEM_RE, file_get_contents($file), $m);
            foreach ($m[1] as $name) {
                $names[$name] = true;
            }
        }

        return $this->itemCache[$key] = $names;
    }

    private function childDir(string $moduleFile, string $child): string
    {
        $base = basename($moduleFile);
        $dir = $base === 'lib.rs' || $base === 'main.rs' ? dirname($moduleFile) : substr($moduleFile, 0, -strlen('.rs'));

        return "{$dir}/{$child}";
    }

    /** @param list<string> $module */
    private function moduleFile(string $crate, array $module): ?string
    {
        $srcDir = $this->crateSrcDirs[$crate];
        if ($module === []) {
            return is_file("{$srcDir}/lib.rs") ? "{$srcDir}/lib.rs" : null;
        }

        $file = $srcDir . '/' . implode('/', $module) . '.rs';

        return is_file($file) ? $file : null;
    }

    /**
     * @return list<array{0: int, 1: string, 2: string, 3: bool, 4: string}>
     *     line, use tree, normalized statement, whether it starts at column 0, preceding attribute line
     */
    private function useStatements(string $path): array
    {
        $statements = [];
        $buffer = null;
        $startIdx = 0;
        $isTopLevel = false;
        $attr = '';
        $prev = '';

        foreach (file($path) as $idx => $raw) {
            $code = explode('//', $raw, 2)[0];
            $stripped = trim($code);

            if ($buffer === null) {
                if (!preg_match(self::USE_START_RE, $stripped)) {
                    if ($stripped !== '') {
                        $prev = $stripped;
                    }
                    continue;
                }
                $buffer = '';
                $startIdx = $idx;
                $isTopLevel = $code !== '' && !ctype_space($code[0]);
                $attr = $prev;
            }

            $buffer .= ' ' . $stripped;
            if (!str_contains($buffer, ';')) {
                continue;
            }

            $statement = trim(preg_replace('/\s+/', ' ', $buffer));
            if (preg_match('/\buse\s+(.*?);/s', $statement, $m)) {
                $statements[] = [$startIdx + 1, $m[1], $statement, $isTopLevel, $attr];
            }
            $buffer = null;
            $prev = $statement;
        }

        return $statements;
    }
}
