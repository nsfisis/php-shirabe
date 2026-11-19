<?php

declare(strict_types=1);

namespace Shirabe\PluginClassifier;

/**
 * The input files under inputs/. Each is a JSON object; an entry that needs a justification
 * carries it in a `reason` field. Shape errors fail the load; whether the entries point at
 * something that exists, and whether they change anything, is checked by the classifier.
 *
 *   two-world.json     {"<namespace prefix or FQCN>": {"reason": "..."}}
 *   static-state.json  {"<FQCN>": {"<property>": "memo-cache|seed-once|needs-sync"}}
 *   overrides.json     {"<FQCN>": {"value": "<value>", "reason": "..."}}
 *   forced.json        {"<FQCN>": {"treatment"?, "reason"?, "members"?, "statics"?,
 *                                  "properties"?, "staticProperties"?}}
 */
final class Inputs
{
    public const DISPOSITIONS = ['memo-cache', 'seed-once', 'needs-sync'];


    public const PROPERTY_HANDLINGS = ['handoff', 'rust-local', 'static-config'];

    /** @var array<string, string> prefix => reason */
    public array $twoWorld = [];

    /** @var array<string, array<string, string>> FQCN => property => disposition */
    public array $staticState = [];

    /** @var array<string, array{value: string, reason: string}> */
    public array $overrides = [];

    /**
     * @var array<string, array{
     *     treatment?: string,
     *     reason?: string,
     *     statics?: array<string, array{status: string, reason: string}>,
     *     properties?: array<string, string>,
     *     staticProperties?: array<string, string>,
     * }>
     */
    public array $forced = [];

    public static function load(string $dir): self
    {
        $inputs = new self();

        foreach (self::read("$dir/two-world.json") as $prefix => $entry) {
            self::expectKeys('two-world.json', $prefix, $entry, ['reason'], ['reason']);
            $inputs->twoWorld[$prefix] = $entry['reason'];
        }

        foreach (self::read("$dir/static-state.json") as $fqcn => $properties) {
            if (!is_array($properties) || $properties === [] || array_is_list($properties)) {
                throw new \RuntimeException("static-state.json: $fqcn: expected an object of properties");
            }
            foreach ($properties as $property => $disposition) {
                if (!in_array($disposition, self::DISPOSITIONS, true)) {
                    throw new \RuntimeException("static-state.json: $fqcn::\$$property: unknown disposition " . json_encode($disposition));
                }
            }
            $inputs->staticState[$fqcn] = $properties;
        }

        foreach (self::read("$dir/overrides.json") as $fqcn => $entry) {
            self::expectKeys('overrides.json', $fqcn, $entry, ['value', 'reason'], ['value', 'reason']);
            if (!in_array($entry['value'], Classifier::VALUES, true)) {
                throw new \RuntimeException("overrides.json: $fqcn: unknown value " . json_encode($entry['value']));
            }
            $inputs->overrides[$fqcn] = $entry;
        }

        foreach (self::read("$dir/forced.json") as $fqcn => $entry) {
            self::expectKeys(
                'forced.json',
                $fqcn,
                $entry,
                ['treatment', 'reason', 'statics', 'properties', 'staticProperties'],
                [],
            );
            if (isset($entry['treatment']) && !in_array($entry['treatment'], Classifier::TREATMENTS, true)) {
                throw new \RuntimeException("forced.json: $fqcn: unknown treatment " . json_encode($entry['treatment']));
            }
            if (isset($entry['reason']) && !isset($entry['treatment'])) {
                throw new \RuntimeException("forced.json: $fqcn: a reason belongs to a treatment");
            }
            foreach ($entry['statics'] ?? [] as $member => $override) {
                self::expectKeys('forced.json', "$fqcn::$member", $override, ['status', 'reason'], ['status', 'reason']);
                if ($override['status'] !== 'forward') {
                    throw new \RuntimeException("forced.json: $fqcn::$member: unknown status " . json_encode($override['status']));
                }
            }
            foreach ($entry['properties'] ?? [] as $property => $handling) {
                if (!in_array($handling, self::PROPERTY_HANDLINGS, true)) {
                    throw new \RuntimeException("forced.json: $fqcn::\$$property: unknown handling " . json_encode($handling));
                }
            }
            foreach ($entry['staticProperties'] ?? [] as $property => $sync) {
                if ($sync !== 'unsynced') {
                    throw new \RuntimeException("forced.json: $fqcn::\$$property: unknown sync " . json_encode($sync));
                }
            }
            $inputs->forced[$fqcn] = $entry;
        }

        return $inputs;
    }

    public function isTwoWorld(string $fqcn): bool
    {
        return $this->twoWorldPrefix($fqcn) !== null;
    }

    public function twoWorldPrefix(string $fqcn): ?string
    {
        foreach (array_keys($this->twoWorld) as $prefix) {
            if ($fqcn === $prefix || str_starts_with($fqcn, rtrim($prefix, '\\') . '\\')) {
                return $prefix;
            }
        }

        return null;
    }

    /** @return array<string, mixed> */
    private static function read(string $path): array
    {
        if (!is_file($path)) {
            throw new \RuntimeException("missing input file: $path");
        }
        $data = json_decode((string) file_get_contents($path), true, 512, JSON_THROW_ON_ERROR);
        if (!is_array($data) || ($data !== [] && array_is_list($data))) {
            throw new \RuntimeException("$path: expected a JSON object");
        }

        return $data;
    }

    /**
     * @param list<string> $allowed
     * @param list<string> $required
     */
    private static function expectKeys(string $file, string $where, mixed $entry, array $allowed, array $required): void
    {
        if (!is_array($entry) || ($entry !== [] && array_is_list($entry))) {
            throw new \RuntimeException("$file: $where: expected an object");
        }
        foreach (array_keys($entry) as $key) {
            if (!in_array($key, $allowed, true)) {
                throw new \RuntimeException("$file: $where: unknown field $key");
            }
        }
        foreach ($required as $key) {
            if (!isset($entry[$key])) {
                throw new \RuntimeException("$file: $where: missing field $key");
            }
        }
        foreach (['reason'] as $key) {
            if (isset($entry[$key]) && (!is_string($entry[$key]) || $entry[$key] === '')) {
                throw new \RuntimeException("$file: $where: $key must be a non-empty string");
            }
        }
    }
}
