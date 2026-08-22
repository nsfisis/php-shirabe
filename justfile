help:
    @just --list

check: lint clippy test

lint: setup
    ./scripts/linters/lint

clippy: setup
    cargo clippy --tests

test: setup
    cargo test

fmt:
    nix fmt

setup: fetch-fixtures
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ ! -f composer/vendor/autoload.php ]]; then
        composer install --no-dev --working-dir=composer
    fi
    if [[ ! -f scripts/linters/vendor/autoload.php ]]; then
        composer install --working-dir=scripts/linters
    fi

fetch-fixtures:
    #!/usr/bin/env bash
    set -euo pipefail
    for fetch in crates/shirabe/tests/plugin/fixtures/*/fetch; do
        "$fetch"
    done
