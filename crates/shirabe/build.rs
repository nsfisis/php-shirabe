//! ref: composer/src/Composer/Compiler.php
//!
//! Generates the constant values of:
//!
//! - `composer::SHIRABE_VERSION`,
//! - `composer::SHIRABE_BRANCH_ALIAS_VERSION`,
//! - `composer::SHIRABE_RELEASE_DATE` and
//! - `composer::COMPOSER_DEV_WARNING_TIME`.

struct Commit {
    hash: String,
    release_date: String,
    time: i64,
}

fn git(repo_root: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .env("TZ", "UTC")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

fn from_env() -> Option<Commit> {
    let hash = std::env::var("SHIRABE_COMMIT_HASH").ok();
    let release_date = std::env::var("SHIRABE_RELEASE_DATE").ok();
    let time = std::env::var("SHIRABE_COMMIT_TIME").ok();
    match (hash, release_date, time) {
        (Some(hash), Some(release_date), Some(time)) => Some(Commit {
            hash,
            release_date,
            time: time
                .parse()
                .expect("SHIRABE_COMMIT_TIME holds the Unix timestamp of the commit"),
        }),
        (None, None, None) => None,
        _ => panic!(
            "SHIRABE_COMMIT_HASH, SHIRABE_RELEASE_DATE and SHIRABE_COMMIT_TIME all describe the \
             commit the build comes from: set them together, or set none and build from a git \
             checkout"
        ),
    }
}

fn from_git() -> Commit {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    if let Some(git_dir) = git(repo_root, &["rev-parse", "--git-dir"]) {
        let git_dir = repo_root.join(git_dir);
        // Committing on the current branch moves the branch ref, not HEAD.
        let branch_ref = git(repo_root, &["symbolic-ref", "-q", "HEAD"]);
        for path in ["HEAD", "packed-refs"]
            .iter()
            .map(|name| git_dir.join(name))
            .chain(branch_ref.map(|name| git_dir.join(name)))
        {
            if path.exists() {
                println!("cargo::rerun-if-changed={}", path.display());
            }
        }
    }

    // These values describe the HEAD commit, so a checkout git cannot read leaves the build
    // unable to date itself. Falling back would ship a build that claims a release date it does
    // not have and that never reports itself as outdated.
    let expect_git = "the release date comes from the HEAD commit: build from a git checkout";
    let hash = git(repo_root, &["rev-parse", "HEAD"]).expect(expect_git);
    let release_date = git(
        repo_root,
        &[
            "log",
            "-n1",
            "--date=format-local:%Y-%m-%d %H:%M:%S",
            "--pretty=%cd",
            "HEAD",
        ],
    )
    .expect(expect_git);
    let time = git(repo_root, &["log", "-n1", "--pretty=%ct", "HEAD"])
        .and_then(|date| date.parse::<i64>().ok())
        .expect(expect_git);

    Commit {
        hash,
        release_date,
        time,
    }
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=SHIRABE_COMMIT_HASH");
    println!("cargo::rerun-if-env-changed=SHIRABE_RELEASE_DATE");
    println!("cargo::rerun-if-env-changed=SHIRABE_COMMIT_TIME");

    let commit = from_env().unwrap_or_else(from_git);

    let package_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let is_dev = std::env::var("CARGO_PKG_VERSION_PRE").unwrap() == "dev";

    // Composer's phar reports a snapshot build as `<branch alias>+<commit hash>` and appends the
    // branch alias to its long version; a tagged release has no branch alias.
    let (version, branch_alias_version, dev_warning_time) = if is_dev {
        (
            format!("{package_version}+{}", commit.hash),
            package_version,
            Some(commit.time + 60 * 86400),
        )
    } else {
        (package_version, String::new(), None)
    };

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("shirabe_version.rs"), format!("{version:?}")).unwrap();
    std::fs::write(
        out_dir.join("shirabe_branch_alias_version.rs"),
        format!("{branch_alias_version:?}"),
    )
    .unwrap();
    std::fs::write(
        out_dir.join("release_date.rs"),
        format!("{:?}", commit.release_date),
    )
    .unwrap();
    std::fs::write(
        out_dir.join("dev_warning_time.rs"),
        match dev_warning_time {
            Some(time) => format!("Some({time})"),
            None => "None".to_string(),
        },
    )
    .unwrap();
}
