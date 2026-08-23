#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
BIN="$REPO_ROOT/target/release/shirabe"
COMPOSER_BIN="$REPO_ROOT/composer/bin/composer"

source "$REPO_ROOT/scripts/bench/lib.sh"

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

apply_http3_workaround

OUTDIR="${TMPDIR:-/tmp}/shirabe-bench-install-$$"
mkdir -p "$OUTDIR"
cd "$OUTDIR"

TARGET_DIR="project-under-test"
PACKAGE_SLUG="${PACKAGE//\//-}"

# Resolve the dependencies once. What is measured is installing them, not solving them.
"$COMPOSER_BIN" create-project --no-plugins --no-scripts --no-audit --no-interaction \
  "$PACKAGE" "$OUTDIR/template" >/dev/null

PREPARE_SCRIPT="$OUTDIR/prepare.sh"
cat > "$PREPARE_SCRIPT" <<EOF
for dir in '$TARGET_DIR-shirabe' '$TARGET_DIR-composer'; do
  rm -rf "\$dir"
  mkdir -p "\$dir"
  cp '$OUTDIR/template/composer.json' '$OUTDIR/template/composer.lock' "\$dir/"
done
EOF

hyperfine \
  --warmup 1 \
  --prepare "bash '$PREPARE_SCRIPT'" \
  --export-json "$OUTDIR/results-$PACKAGE_SLUG.json" \
  --export-markdown "$OUTDIR/results-$PACKAGE_SLUG.md" \
  --command-name Shirabe "RUST_BACKTRACE=1 '$BIN' install --no-plugins --no-scripts --no-interaction --working-dir='$TARGET_DIR-shirabe'" \
  --command-name Composer "'$COMPOSER_BIN' install --no-plugins --no-scripts --no-interaction --working-dir='$TARGET_DIR-composer'"

echo ">> results: $OUTDIR/results-$PACKAGE_SLUG.json" >&2
echo ">>          $OUTDIR/results-$PACKAGE_SLUG.md" >&2
