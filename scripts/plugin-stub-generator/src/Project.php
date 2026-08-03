<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

use PhpParser\Parser;
use PhpParser\ParserFactory;

/**
 * Locates and parses classes of the Composer checkout (composer/src and composer/vendor),
 * using the checkout's own PSR-4 autoload map.
 */
final class Project
{
    private Parser $parser;

    /** @var array<string, list<string>> namespace prefix => base dirs, longest prefix first */
    private array $psr4;

    /** @var array<string, SourceFile> */
    private array $cache = [];

    public function __construct(string $composerRoot)
    {
        $this->parser = (new ParserFactory())->createForNewestSupportedVersion();

        $mapFile = $composerRoot . '/vendor/composer/autoload_psr4.php';
        if (!is_file($mapFile)) {
            throw new GenerationError(["missing $mapFile (run composer install in the checkout)"]);
        }
        $map = require $mapFile;
        $map['Composer\\'] ??= [$composerRoot . '/src/Composer'];
        uksort($map, static fn (string $a, string $b): int => strlen($b) <=> strlen($a));
        $this->psr4 = $map;
    }

    public function sourceFor(string $fqcn): SourceFile
    {
        return $this->cache[$fqcn] ??= new SourceFile($this->fileFor($fqcn), $fqcn, $this->parser);
    }

    private function fileFor(string $fqcn): string
    {
        foreach ($this->psr4 as $prefix => $dirs) {
            if (!str_starts_with($fqcn, $prefix)) {
                continue;
            }
            $relative = str_replace('\\', '/', substr($fqcn, strlen($prefix))) . '.php';
            foreach ($dirs as $dir) {
                $path = $dir . '/' . $relative;
                if (is_file($path)) {
                    return $path;
                }
            }
        }
        throw new GenerationError(["cannot locate a source file for $fqcn"]);
    }
}
