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
        $allPassed = true;

        foreach ($this->linters as [$linter, $excludes]) {
            echo "===== {$linter->name()} =====\n";

            $errors = $linter->check($this->rootDir, $excludes);
            if ($errors === []) {
                echo "Passed.\n";
            } else {
                echo $linter->failureIntro(), "\n";
                foreach ($errors as $error) {
                    echo "  {$error}\n";
                }
                $allPassed = false;
            }

            echo "\n";
        }

        return $allPassed;
    }
}
