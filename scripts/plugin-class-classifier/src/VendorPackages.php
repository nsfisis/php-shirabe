<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * Package-level classification of composer/composer's runtime vendor
 * dependencies, applied by namespace prefix. See the "Mechanical rules"
 * step 6 in docs/dev/plugin-class-classification.md for the criterion.
 */
final class VendorPackages
{
    /** @var array<string, array{package: string, category: string}> prefix => info */
    public const PREFIXES = [
        'Composer\\Semver\\' => ['package' => 'composer/semver', 'category' => 'php-native'],
        'Composer\\Pcre\\' => ['package' => 'composer/pcre', 'category' => 'php-native'],
        'Composer\\CaBundle\\' => ['package' => 'composer/ca-bundle', 'category' => 'php-native'],
        'Composer\\ClassMapGenerator\\' => ['package' => 'composer/class-map-generator', 'category' => 'php-native'],
        'Composer\\MetadataMinifier\\' => ['package' => 'composer/metadata-minifier', 'category' => 'php-native'],
        'Composer\\Spdx\\' => ['package' => 'composer/spdx-licenses', 'category' => 'php-native'],
        'Composer\\XdebugHandler\\' => ['package' => 'composer/xdebug-handler', 'category' => 'php-native'],
        'JsonSchema\\' => ['package' => 'justinrainbow/json-schema', 'category' => 'php-native'],
        'Seld\\JsonLint\\' => ['package' => 'seld/jsonlint', 'category' => 'php-native'],
        'Seld\\PharUtils\\' => ['package' => 'seld/phar-utils', 'category' => 'php-native'],
        'Seld\\Signal\\' => ['package' => 'seld/signal-handler', 'category' => 'php-native'],
        'Psr\\Log\\' => ['package' => 'psr/log', 'category' => 'php-native'],
        'React\\Promise\\' => ['package' => 'react/promise', 'category' => 'contract'],
        'Symfony\\Component\\Console\\' => ['package' => 'symfony/console', 'category' => 'two-world'],
        'Symfony\\Component\\Process\\' => ['package' => 'symfony/process', 'category' => 'php-native'],
        'Symfony\\Component\\Filesystem\\' => ['package' => 'symfony/filesystem', 'category' => 'php-native'],
        'Symfony\\Component\\Finder\\' => ['package' => 'symfony/finder', 'category' => 'php-native'],
        'Symfony\\Polyfill\\' => ['package' => 'symfony/polyfill', 'category' => 'php-native'],
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

    /** @return array{package: string, category: string}|null */
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
