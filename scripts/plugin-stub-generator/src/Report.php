<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/**
 * The classifier report (scripts/plugin-class-classifier/report.json), the only input besides the
 * Composer sources: it names the treatment of every class and the members each worker-side
 * definition forwards.
 */
final class Report
{
    /**
     * @param array<string, array<string, mixed>> $classes fqcn => row
     */
    private function __construct(private readonly array $classes)
    {
    }

    public static function load(string $path): self
    {
        if (!is_file($path)) {
            throw new GenerationError([
                "missing classifier report $path (run scripts/plugin-class-classifier/classify first)",
            ]);
        }
        $data = json_decode((string) file_get_contents($path), true, 512, JSON_THROW_ON_ERROR);
        if (($data['violations'] ?? null) !== []) {
            throw new GenerationError(["$path records classification violations; fix the classifier inputs first"]);
        }
        $classes = [];
        foreach ($data['classes'] as $class) {
            $classes[$class['fqcn']] = $class;
        }
        return new self($classes);
    }

    public function treatment(string $fqcn): ?string
    {
        return $this->classes[$fqcn]['treatment'] ?? null;
    }

    public function kind(string $fqcn): ?string
    {
        return $this->classes[$fqcn]['kind'] ?? null;
    }

    public function parent(string $fqcn): ?string
    {
        foreach ($this->classes[$fqcn]['hardRefs'] ?? [] as $ref) {
            if ($ref['kind'] === 'extends') {
                return $ref['target'];
            }
        }
        return null;
    }

    /**
     * The FQCNs of a treatment, in report order.
     *
     * @return list<string>
     */
    public function withTreatment(string $treatment): array
    {
        $out = [];
        foreach ($this->classes as $fqcn => $class) {
            if ($class['treatment'] === $treatment) {
                $out[] = $fqcn;
            }
        }
        return $out;
    }

    /**
     * The FQCNs of a treatment, every class after its ancestors.
     *
     * @return list<string>
     */
    public function withTreatmentParentsFirst(string $treatment): array
    {
        $out = [];
        $placed = [];
        $place = function (string $fqcn) use (&$place, &$out, &$placed, $treatment): void {
            if (isset($placed[$fqcn])) {
                return;
            }
            $placed[$fqcn] = true;
            $parent = $this->parent($fqcn);
            if ($parent !== null && $this->treatment($parent) === $treatment) {
                $place($parent);
            }
            $out[] = $fqcn;
        };
        foreach ($this->withTreatment($treatment) as $fqcn) {
            $place($fqcn);
        }
        return $out;
    }

    /**
     * What the worker-side definition of a class forwards to the Rust side.
     *
     * @return list<array{member: string, kind: string, reason?: string}>
     */
    public function forwarded(string $fqcn): array
    {
        return $this->classes[$fqcn]['forwarded'] ?? [];
    }

    /** @return list<string> the classes that forward members, in report order */
    public function forwarding(): array
    {
        $out = [];
        foreach ($this->classes as $fqcn => $class) {
            if (isset($class['forwarded'])) {
                $out[] = $fqcn;
            }
        }
        return $out;
    }

    /**
     * The static properties of a class the worker and the Rust side share, with how each is kept
     * in sync.
     *
     * @return list<array{name: string, disposition: string, sync: ?string}>
     */
    public function staticProperties(string $fqcn): array
    {
        return $this->classes[$fqcn]['staticProperties'] ?? [];
    }

    /** @return list<string> every class of the report, in report order */
    public function classes(): array
    {
        return array_keys($this->classes);
    }

    public function isAbstract(string $fqcn): bool
    {
        return $this->classes[$fqcn]['abstract'] ?? false;
    }
}
