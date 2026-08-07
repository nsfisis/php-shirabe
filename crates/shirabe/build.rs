//! ref: composer/src/Composer/Compiler.php
//!
//! Generates a constant value of `composer::COMPOSER_DEV_WARNING_TIME`.

fn git(repo_root: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    if let Some(git_dir) = git(repo_root, &["rev-parse", "--git-dir"]) {
        let git_dir = repo_root.join(git_dir);
        for path in ["HEAD", "packed-refs", "refs/tags"] {
            let path = git_dir.join(path);
            if path.exists() {
                println!("cargo::rerun-if-changed={}", path.display());
            }
        }
    }

    let dev_warning_time =
        if git(repo_root, &["describe", "--tags", "--exact-match", "HEAD"]).is_some() {
            None
        } else {
            git(repo_root, &["log", "-n1", "--pretty=%ct", "HEAD"])
                .and_then(|date| date.parse::<i64>().ok())
                .map(|date| date + 60 * 86400)
        };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(
        std::path::Path::new(&out_dir).join("dev_warning_time.rs"),
        match dev_warning_time {
            Some(time) => format!("Some({time})"),
            None => "None".to_string(),
        },
    )
    .unwrap();
}
