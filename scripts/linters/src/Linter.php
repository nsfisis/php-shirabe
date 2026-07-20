<?php

declare(strict_types=1);

namespace Shirabe\Lint;

interface Linter
{
    public function name(): string;

    /**
     * @param list<string> $excludes root-relative paths to skip
     * @return list<string> formatted violation lines; empty when the linter passes
     */
    public function check(string $rootDir, array $excludes): array;

    /** Printed once, above the violation list, when violations are found. */
    public function failureIntro(): string;
}
