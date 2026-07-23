#!/usr/bin/env bash

# Work around https://github.com/composer/composer/issues/12987:
apply_http3_workaround() {
  git -C "$REPO_ROOT/composer" apply "$REPO_ROOT/scripts/bench/disable-http3.patch"
  trap 'git -C "$REPO_ROOT/composer" restore -- src/Composer/Util/Http/CurlDownloader.php' EXIT
}
