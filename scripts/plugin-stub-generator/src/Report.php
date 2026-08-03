<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/** The classifier report (scripts/plugin-class-classifier/report.json). */
final class Report
{
    /** @param array<string, string> $categories fqcn => category */
    private function __construct(private readonly array $categories)
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
        foreach ($data['classes'] as $class) {
            $categories[$class['fqcn']] = $class['category'];
        }
        return new self($categories);
    }

    public function category(string $fqcn): ?string
    {
        return $this->categories[$fqcn] ?? null;
    }
}
