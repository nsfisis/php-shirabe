#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
BIN="$REPO_ROOT/target/release/shirabe"

PACKAGE="laravel/laravel"
for arg in "$@"; do
  case "$arg" in
    --package=*) PACKAGE="${arg#*=}" ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine not found; install it first (e.g. 'cargo install hyperfine')" >&2
  exit 1
fi

cargo build --manifest-path "$REPO_ROOT/Cargo.toml" --release --bin shirabe

OUTDIR="${TMPDIR:-/tmp}/shirabe-bench-create-project-$$"
mkdir -p "$OUTDIR"
cd "$OUTDIR"

TARGET_DIR="project-under-test"
PACKAGE_SLUG="${PACKAGE//\//-}"

hyperfine \
  --warmup 1 \
  --prepare "rm -rf '$TARGET_DIR-shirabe' '$TARGET_DIR-composer'" \
  --export-json "$OUTDIR/results-$PACKAGE_SLUG.json" \
  --export-markdown "$OUTDIR/results-$PACKAGE_SLUG.md" \
  --ignore-failure \
  --command-name Shirabe "'$BIN' create-project --profile --no-plugins --no-scripts '$PACKAGE' '$TARGET_DIR-shirabe'" \
  --command-name Composer "composer create-project --profile --no-plugins --no-scripts '$PACKAGE' '$TARGET_DIR-composer'"

echo ">> results: $OUTDIR/results-$PACKAGE_SLUG.json" >&2
echo ">>          $OUTDIR/results-$PACKAGE_SLUG.md" >&2
