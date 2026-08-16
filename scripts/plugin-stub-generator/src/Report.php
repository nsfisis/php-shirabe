<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/** The classifier report (scripts/plugin-class-classifier/report.json). */
final class Report
{
    /**
     * @param array<string, string> $categories fqcn => category
     * @param array<string, string> $kinds      fqcn => class / interface / trait / enum
     */
    private function __construct(private readonly array $categories, private readonly array $kinds)
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
            throw new GenerationError(["$path records classification violations; fix the classifier lists first"]);
        }
        $categories = [];
        $kinds = [];
        foreach ($data['classes'] as $class) {
            $categories[$class['fqcn']] = $class['category'];
            $kinds[$class['fqcn']] = $class['kind'];
        }
        return new self($categories, $kinds);
    }

    public function category(string $fqcn): ?string
    {
        return $this->categories[$fqcn] ?? null;
    }

    public function kind(string $fqcn): ?string
    {
        return $this->kinds[$fqcn] ?? null;
    }

    /**
     * The FQCNs of the given categories, in report order.
     *
     * @param  list<string> $categories
     * @return list<string>
     */
    public function inCategories(array $categories): array
    {
        $out = [];
        foreach ($this->categories as $fqcn => $category) {
            if (in_array($category, $categories, true)) {
                $out[] = $fqcn;
            }
        }
        return $out;
    }
}
