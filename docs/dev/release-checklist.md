# Release checklist

`X.Y.Z` is the version of Shirabe being released, and `A.B.C` is the corresponding Composer version.

## Preparation

1. Check the preconditions:
    1. You are on the `main` branch.
    2. `main` and `origin/main` point to the same commit.
    3. The source tree is clean.
    4. All GitHub Actions CI runs pass on `origin/main`.
2. Review the whole diff since the previous release, and fill in anything missing from the CHANGELOG.
    - `git log <previous tag>..HEAD --format=%s`
3. Rewrite `## Unreleased (based on Composer A.B.C)` in the CHANGELOG to `## vX.Y.Z (based on Composer A.B.C)`.
4. Remove `-dev` from `[workspace.package] version` in `Cargo.toml`.
5. Update `Cargo.lock` with `cargo metadata --offline --format-version 1 >/dev/null`, and check that nothing other than the versions of the workspace crates has changed.

## Commit

1. Commit: `feat: bump version to X.Y.Z`
2. Run `git push origin main`, and wait for the Test workflow (`.github/workflows/test.yaml`) to pass.
    - `gh run list --branch main --limit 3` → `gh run watch <id>`

## Tag

1. Pass the same checks locally as the build job of the Release workflow.
2. Create the tag.
    - `git tag -a X.Y.Z -m "Release X.Y.Z (based on Composer A.B.C)"`
3. `git push origin X.Y.Z`
4. `gh run list --workflow release.yaml --limit 1` → `gh run watch <id>`
5. If it fails:
    - If the release job fails partway, a draft release may be left behind. Check with `gh release list`, and delete it after fixing the problem, before re-running.
    - If the fix requires a commit, either recreate the tag or release X.Y.Z+1 instead.
        - To recreate the tag: commit the fix, then `git tag -f -a X.Y.Z -m "..."` → `git push origin main` → `git push --force origin X.Y.Z`

## Verification

1. With `gh release view X.Y.Z`, check the `.tar.gz` and `.tar.gz.sha256` for all 3 targets, and the release notes.
2. Download the release binary for the current machine.
    - `sha256sum -c`
    - `gh attestation verify <tarball> --repo nsfisis/php-shirabe`
    - `shirabe --version`

## Post-release

1. Set the version in `Cargo.toml` to the next development version (with `-dev`), and update `Cargo.lock`.
2. Add `## Unreleased (based on Composer A.B.C)` at the top of the CHANGELOG.
3. Commit (`chore: bump version to X.(Y+1).0-dev`) and push.
