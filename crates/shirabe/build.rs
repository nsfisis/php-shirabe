//! ref: composer/src/Composer/Compiler.php sha1:9c0d11aa05bfdbfee70f2fc21cbf72ec57101496
//!
//! Generates the constant values of:
//!
//! - `composer::SHIRABE_VERSION`,
//! - `composer::SHIRABE_BRANCH_ALIAS_VERSION`,
//! - `composer::SHIRABE_RELEASE_DATE` and
//! - `composer::COMPOSER_DEV_WARNING_TIME`,
//!
//! and builds the Composer PHP runtime phar bundle embedded in the distributed binary. See also
//! `docs/dev/composer-runtime-bundle.md`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "the file generates Rust source, which rustc reads as text"
)]

use std::io::Write as _;

/// The directory names Symfony's `Finder::ignoreVCS(true)` excludes.
const VCS_DIRECTORIES: &[&str] = &[
    ".svn",
    "_svn",
    "CVS",
    "_darcs",
    ".arch-params",
    ".monotone",
    ".bzr",
    ".git",
    ".hg",
];

/// Directories `Compiler::compile` excludes from the vendor tree, on top of the VCS ones.
const VENDOR_EXCLUDED_DIRECTORIES: &[&str] = &["Tests", "tests", "docs"];

/// Vendor paths `Compiler::compile` expects in the archive even though they are not PHP sources
/// or licenses; a missing one means the source package changed under us.
const VENDOR_EXTRA_FILES: &[&str] = &[
    "composer/installed.json",
    "composer/spdx-licenses/res/spdx-exceptions.json",
    "composer/spdx-licenses/res/spdx-licenses.json",
    "composer/ca-bundle/res/cacert.pem",
    "symfony/console/Resources/bin/hiddeninput.exe",
    "symfony/console/Resources/completion.bash",
];

/// The path of the entry whose read-back tells the worker that the bundle is usable.
const SENTINEL_PATH: &str = "shirabe/bundle-id";

struct Entry {
    /// Path inside the archive, relative to the Composer checkout root.
    path: String,
    content: Vec<u8>,
}

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
    Some(
        output
            .stdout
            .iter()
            .map(|byte| char::from(*byte))
            .collect::<String>()
            .trim()
            .to_string(),
    )
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

    build_bundle(&out_dir);
}

/// Builds the Composer PHP runtime bundle into `out_dir` and names it in
/// `SHIRABE_COMPOSER_RUNTIME_BUNDLE_ID`.
fn build_bundle(out_dir: &std::path::Path) {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let composer = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("composer");
    for path in ["src", "res", "vendor", "bin/composer", "LICENSE"] {
        println!("cargo::rerun-if-changed={}", composer.join(path).display());
    }

    if !composer.join("vendor/autoload.php").is_file() {
        panic!(
            "the Composer PHP runtime is missing from {}; run `git submodule update --init` \
             and `composer install` in it",
            composer.display()
        );
    }

    let mut entries = collect_entries(&composer);
    let bundle_id = bundle_id(&entries);
    entries.push(Entry {
        path: SENTINEL_PATH.to_string(),
        content: bundle_id.clone().into_bytes(),
    });

    std::fs::write(
        out_dir.join("composer-runtime-bundle.phar"),
        build_phar(&entries),
    )
    .unwrap();
    println!("cargo::rustc-env=SHIRABE_COMPOSER_RUNTIME_BUNDLE_ID={bundle_id}");
}

/// Every file the bundle holds, in the order `Compiler::compile` adds them.
fn collect_entries(composer: &std::path::Path) -> Vec<Entry> {
    let mut entries = Vec::new();

    // Add Composer sources. Compiler.php excludes ClassLoader.php and InstalledVersions.php from
    // this pass only to add them back unstripped; nothing here strips whitespace, so they are
    // taken along with the rest.
    for file in find_files(&composer.join("src")) {
        if file.extension() != Some(std::ffi::OsStr::new("php"))
            || file_name(&file) == "Compiler.php"
        {
            continue;
        }
        entries.push(read_entry(composer, &file, true));
    }

    // Add Composer resources.
    for file in find_files(&composer.join("res")) {
        entries.push(read_entry(composer, &file, false));
    }

    // Add vendor files.
    let mut extra_files: Vec<&str> = VENDOR_EXTRA_FILES.to_vec();
    let mut unexpected_files = Vec::new();
    let vendor = composer.join("vendor");
    for file in find_files(&vendor) {
        let relative = relative_path(&vendor, &file);
        if is_excluded_vendor_path(&relative) {
            continue;
        }
        if let Some(index) = extra_files.iter().position(|extra| *extra == relative) {
            extra_files.remove(index);
        } else if !is_license_or_php(&file_name(&file)) {
            unexpected_files.push(relative.clone());
        }
        let strip = is_php_source(&file_name(&file));
        entries.push(read_entry(composer, &file, strip));
    }
    if !extra_files.is_empty() {
        panic!(
            "these files were expected but not added to the phar, they might be excluded or gone \
             from the source package: {extra_files:?}"
        );
    }
    if !unexpected_files.is_empty() {
        panic!(
            "these files were unexpectedly added to the phar, make sure they are excluded or \
             listed in VENDOR_EXTRA_FILES: {unexpected_files:?}"
        );
    }

    // Add bin/composer.
    let bin = std::fs::read(composer.join("bin/composer")).unwrap();
    let shebang = b"#!/usr/bin/env php";
    let content = match bin.strip_prefix(&shebang[..]) {
        Some(rest) => {
            let trimmed = rest
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(rest.len());
            rest[trimmed..].to_vec()
        }
        None => bin,
    };
    entries.push(Entry {
        path: "bin/composer".to_string(),
        content,
    });

    entries.push(read_entry(composer, &composer.join("LICENSE"), false));

    entries
}

/// Reads one file the way `Compiler::addFile` does, minus its whitespace stripping: the bundle
/// keeps the sources byte for byte, so a class the worker loads from here is the same file the
/// Rust side embeds elsewhere.
fn read_entry(composer: &std::path::Path, file: &std::path::Path, strip: bool) -> Entry {
    let mut content = std::fs::read(file).unwrap();
    if !strip && file_name(file) == "LICENSE" {
        content.insert(0, b'\n');
        content.push(b'\n');
    }
    Entry {
        path: relative_path(composer, file),
        content,
    }
}

/// The `notPath`/`exclude` rules `Compiler::compile` applies to the vendor tree, against a path
/// relative to `vendor/`.
fn is_excluded_vendor_path(relative: &str) -> bool {
    // Every rule wants a `/` in front of the file name, so a file sitting directly in `vendor/`
    // (autoload.php) is never matched.
    let Some((directories, name)) = relative.rsplit_once('/') else {
        return false;
    };
    if directories
        .split('/')
        .any(|segment| VENDOR_EXCLUDED_DIRECTORIES.contains(&segment))
    {
        return true;
    }
    for substring in [
        "justinrainbow/json-schema/demo/",
        "justinrainbow/json-schema/dist/",
        "justinrainbow/json-schema/bin/",
        "composer/pcre/extension.neon",
        "composer/LICENSE",
    ] {
        if relative.contains(substring) {
            return true;
        }
    }
    if name.starts_with("UPGRADE") && (name.ends_with(".md") || name.ends_with(".txt")) {
        return true;
    }
    if name.ends_with(".md") || name.ends_with(".mdown") {
        let stem = name.trim_end_matches(".mdown").trim_end_matches(".md");
        if !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return true;
        }
    }
    matches!(
        name,
        "composer.json"
            | "composer.lock"
            | ".gitignore"
            | "appveyor.yml"
            | "phpunit.xml.dist"
            | "phpstan.neon.dist"
            | "phpstan-config.neon"
            | "phpstan-baseline.neon"
    ) || matches!(
        relative.rsplit_once("bin/").map(|(_, name)| name),
        Some(
            "jsonlint"
                | "validate-json"
                | "simple-phpunit"
                | "phpstan"
                | "phpstan.phar"
                | "jsonlint.bat"
                | "validate-json.bat"
                | "simple-phpunit.bat"
                | "phpstan.bat"
                | "phpstan.phar.bat"
        )
    )
}

/// `Compiler::compile`'s guard against silently packing something that is neither a license nor a
/// PHP source.
fn is_license_or_php(name: &str) -> bool {
    name == "LICENSE" || name == "LICENSE.txt" || name.ends_with(".php")
}

/// Whether `Compiler::compile` treats a vendor file as PHP source (`{\.php[\d.]*$}`).
fn is_php_source(name: &str) -> bool {
    match name.rsplit_once(".php") {
        Some((_, suffix)) => suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.'),
        None => false,
    }
}

/// Identifies the bundle by its contents, naming both the sentinel entry and the directory the
/// Rust side extracts to when the worker cannot read the bundle in place.
fn bundle_id(entries: &[Entry]) -> String {
    use sha2::Digest as _;

    let mut hasher = sha2::Sha256::new();
    for entry in entries {
        hasher.update((entry.path.len() as u64).to_le_bytes());
        hasher.update(entry.path.as_bytes());
        hasher.update((entry.content.len() as u64).to_le_bytes());
        hasher.update(&entry.content);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const PHAR_FILE_COMPRESSED_GZ: u32 = 0x0000_1000;

/// Lays out an unsigned native phar. Unsigned because a phar signature covers everything from the
/// start of the file, which for an embedded bundle means bytes the linker has not produced yet.
fn build_phar(entries: &[Entry]) -> Vec<u8> {
    let mut manifest = Vec::new();
    let mut contents = Vec::new();
    let mut global_flags = 0u32;

    for entry in entries {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&entry.content).unwrap();
        let deflated = encoder.finish().unwrap();
        let (data, flags) = if deflated.len() < entry.content.len() {
            global_flags |= PHAR_FILE_COMPRESSED_GZ;
            (deflated, 0o644 | PHAR_FILE_COMPRESSED_GZ)
        } else {
            (entry.content.clone(), 0o644)
        };

        manifest.extend_from_slice(&(entry.path.len() as u32).to_le_bytes());
        manifest.extend_from_slice(entry.path.as_bytes());
        manifest.extend_from_slice(&(entry.content.len() as u32).to_le_bytes());
        // A fixed timestamp, so that the same checkout always produces the same bundle.
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&(data.len() as u32).to_le_bytes());
        manifest.extend_from_slice(&crc32(&entry.content).to_le_bytes());
        manifest.extend_from_slice(&flags.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        contents.extend_from_slice(&data);
    }

    let mut header = Vec::new();
    header.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    // API version 1.1.0, the only field phar reads big-endian.
    header.extend_from_slice(&[0x11, 0x00]);
    header.extend_from_slice(&global_flags.to_le_bytes());
    // Neither an alias nor metadata: the alias is the one the worker passes to Phar::loadPhar.
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());

    let mut bytes = b"<?php __HALT_COMPILER(); ?>\r\n".to_vec();
    bytes.extend_from_slice(&((header.len() + manifest.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&manifest);
    bytes.extend_from_slice(&contents);
    bytes
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    crc.sum()
}

/// The files under `root`, sorted by path, with the entries Symfony's `Finder` skips by default
/// (dot files and VCS directories) left out.
fn find_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    files
}

fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = file_name(&path);
        if name.starts_with('.') {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            if VCS_DIRECTORIES.contains(&name.as_str()) {
                continue;
            }
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().unwrap().display().to_string()
}

fn relative_path(root: &std::path::Path, file: &std::path::Path) -> String {
    file.strip_prefix(root).unwrap().display().to_string()
}
