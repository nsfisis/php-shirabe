<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * composer/composer's runtime vendor dependencies, applied by namespace
 * prefix. A vendor package is handled as a whole: symfony/console is
 * two-world, every other one runs as real code in the worker (`worker`).
 * The vendor classes that cross by value are named in overrides.json.
 *
 * `valueSafe` says whether a value object may hold the package's types in
 * its state: false for the packages whose types are living objects
 * (promises, console objects) rather than values.
 */
final class VendorPackages
{
    /** @var array<string, array{package: string, value: string, valueSafe: bool}> prefix => info */
    public const PREFIXES = [
        'Composer\\Semver\\' => ['package' => 'composer/semver', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\Pcre\\' => ['package' => 'composer/pcre', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\CaBundle\\' => ['package' => 'composer/ca-bundle', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\ClassMapGenerator\\' => ['package' => 'composer/class-map-generator', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\MetadataMinifier\\' => ['package' => 'composer/metadata-minifier', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\Spdx\\' => ['package' => 'composer/spdx-licenses', 'value' => 'worker', 'valueSafe' => true],
        'Composer\\XdebugHandler\\' => ['package' => 'composer/xdebug-handler', 'value' => 'worker', 'valueSafe' => true],
        'JsonSchema\\' => ['package' => 'justinrainbow/json-schema', 'value' => 'worker', 'valueSafe' => true],
        'Seld\\JsonLint\\' => ['package' => 'seld/jsonlint', 'value' => 'worker', 'valueSafe' => true],
        'Seld\\PharUtils\\' => ['package' => 'seld/phar-utils', 'value' => 'worker', 'valueSafe' => true],
        'Seld\\Signal\\' => ['package' => 'seld/signal-handler', 'value' => 'worker', 'valueSafe' => true],
        'Psr\\Log\\' => ['package' => 'psr/log', 'value' => 'worker', 'valueSafe' => true],
        'React\\Promise\\' => ['package' => 'react/promise', 'value' => 'worker', 'valueSafe' => false],
        'Symfony\\Component\\Console\\' => ['package' => 'symfony/console', 'value' => 'two-world', 'valueSafe' => false],
        'Symfony\\Component\\Process\\' => ['package' => 'symfony/process', 'value' => 'worker', 'valueSafe' => true],
        'Symfony\\Component\\Filesystem\\' => ['package' => 'symfony/filesystem', 'value' => 'worker', 'valueSafe' => true],
        'Symfony\\Component\\Finder\\' => ['package' => 'symfony/finder', 'value' => 'worker', 'valueSafe' => true],
        'Symfony\\Polyfill\\' => ['package' => 'symfony/polyfill', 'value' => 'worker', 'valueSafe' => true],
    ];

    /**
     * By-ref parameter positions of vendor methods, needed by the purity
     * analysis when composer code passes $this-rooted expressions to them.
     * Vendor packages other than composer/pcre expose no by-ref parameters
     * in APIs composer calls; composer/pcre's output parameter is pervasive.
     *
     * @var array<string, array<string, list<int>>> FQCN => lowercase method => positions
     */
    public const BY_REF = [
        'Composer\\Pcre\\Preg' => [
            'match' => [2],
            'matchstrictgroups' => [2],
            'ismatch' => [2],
            'ismatchstrictgroups' => [2],
            'matchall' => [2],
            'matchallstrictgroups' => [2],
            'ismatchall' => [2],
            'ismatchallstrictgroups' => [2],
        ],
        'Composer\\Pcre\\Regex' => [],
    ];

    /** @return array{package: string, value: string, valueSafe: bool}|null */
    public static function lookup(string $fqcn): ?array
    {
        foreach (self::PREFIXES as $prefix => $info) {
            if (str_starts_with($fqcn, $prefix)) {
                return $info;
            }
        }

        return null;
    }
}
