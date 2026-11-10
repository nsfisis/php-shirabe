<?php

declare(strict_types=1);

namespace Shirabe\Lint;

final class Runner
{
    /** @param list<array{0: Linter, 1: list<string>}> $linters */
    public function __construct(
        private readonly string $rootDir,
        private readonly array $linters,
    ) {
    }

    public function run(): bool
    {
        $failed = [];

        foreach ($this->linters as [$linter, $excludes]) {
            $errors = $linter->check($this->rootDir, $excludes);
            if ($errors === []) {
                continue;
            }

            echo "===== {$linter->name()} =====\n";
            echo "FAILED.\n";
            echo $linter->failureIntro(), "\n";
            foreach ($errors as $error) {
                echo "  {$error}\n";
            }
            echo "\n";
            $failed[] = $linter->name();
        }

        $total = count($this->linters);
        if ($failed === []) {
            echo "PASSED: All {$total} lint rules passed.\n";
        } else {
            $count = count($failed);
            echo "FAILED: {$count} of {$total} lint rules failed: ", implode(', ', $failed), "\n";
        }

        return $failed === [];
    }
}
