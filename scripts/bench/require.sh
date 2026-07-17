#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
BIN="$REPO_ROOT/target/release/shirabe"

PACKAGE="monolog/monolog"
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

OUTDIR="${TMPDIR:-/tmp}/shirabe-bench-require-$$"
mkdir -p "$OUTDIR"
cd "$OUTDIR"

TARGET_DIR="project-under-test"
PACKAGE_SLUG="${PACKAGE//\//-}"

PREPARE_SCRIPT="$OUTDIR/prepare.sh"
cat > "$PREPARE_SCRIPT" <<EOF
rm -rf '$TARGET_DIR-shirabe' '$TARGET_DIR-composer'
mkdir -p '$TARGET_DIR-shirabe' '$TARGET_DIR-composer'
printf '{"name": "shirabe-bench/require-test"}' > '$TARGET_DIR-shirabe/composer.json'
printf '{"name": "shirabe-bench/require-test"}' > '$TARGET_DIR-composer/composer.json'
EOF

hyperfine \
  --warmup 1 \
  --prepare "bash '$PREPARE_SCRIPT'" \
  --export-json "$OUTDIR/results-$PACKAGE_SLUG.json" \
  --export-markdown "$OUTDIR/results-$PACKAGE_SLUG.md" \
  --ignore-failure \
  "'$BIN' require --no-install --no-audit --no-interaction --working-dir='$TARGET_DIR-shirabe' '$PACKAGE'" \
  "composer require --no-install --no-audit --no-interaction --working-dir='$TARGET_DIR-composer' '$PACKAGE'"

echo ">> results: $OUTDIR/results-$PACKAGE_SLUG.json" >&2
echo ">>          $OUTDIR/results-$PACKAGE_SLUG.md" >&2
