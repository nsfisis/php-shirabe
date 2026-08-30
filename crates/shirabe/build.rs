//! ref: composer/src/Composer/Compiler.php
//!
//! Generates the constant values of `composer::SHIRABE_RELEASE_DATE` and
//! `composer::COMPOSER_DEV_WARNING_TIME` from the HEAD commit.

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

fn from_env() -> Option<(String, Option<i64>)> {
    let release_date = std::env::var("SHIRABE_RELEASE_DATE").ok();
    let dev_warning_time = std::env::var("SHIRABE_DEV_WARNING_TIME").ok();
    match (release_date, dev_warning_time) {
        (Some(release_date), Some(dev_warning_time)) => {
            let dev_warning_time = if dev_warning_time.is_empty() {
                None
            } else {
                Some(dev_warning_time.parse().expect(
                    "SHIRABE_DEV_WARNING_TIME holds a Unix timestamp, or nothing at all for a \
                     tagged release",
                ))
            };
            Some((release_date, dev_warning_time))
        }
        (None, None) => None,
        _ => panic!(
            "SHIRABE_RELEASE_DATE and SHIRABE_DEV_WARNING_TIME both describe the commit the build \
             comes from: set them together, or set neither and build from a git checkout"
        ),
    }
}

fn from_git() -> (String, Option<i64>) {
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
        for path in ["HEAD", "packed-refs", "refs/tags"]
            .iter()
            .map(|name| git_dir.join(name))
            .chain(branch_ref.map(|name| git_dir.join(name)))
        {
            if path.exists() {
                println!("cargo::rerun-if-changed={}", path.display());
            }
        }
    }

    // Both constants describe the HEAD commit, so a checkout git cannot read leaves the build
    // unable to date itself. Falling back would ship a build that claims a release date it does
    // not have and that never reports itself as outdated.
    let expect_git = "the release date comes from the HEAD commit: build from a git checkout";
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
    let commit_time = git(repo_root, &["log", "-n1", "--pretty=%ct", "HEAD"])
        .and_then(|date| date.parse::<i64>().ok())
        .expect(expect_git);

    let dev_warning_time =
        if git(repo_root, &["describe", "--tags", "--exact-match", "HEAD"]).is_some() {
            None
        } else {
            Some(commit_time + 60 * 86400)
        };

    (release_date, dev_warning_time)
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=SHIRABE_RELEASE_DATE");
    println!("cargo::rerun-if-env-changed=SHIRABE_DEV_WARNING_TIME");

    let (release_date, dev_warning_time) = match from_env() {
        Some(values) => values,
        None => from_git(),
    };

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("release_date.rs"), format!("{release_date:?}")).unwrap();
    std::fs::write(
        out_dir.join("dev_warning_time.rs"),
        match dev_warning_time {
            Some(time) => format!("Some({time})"),
            None => "None".to_string(),
        },
    )
    .unwrap();
}
