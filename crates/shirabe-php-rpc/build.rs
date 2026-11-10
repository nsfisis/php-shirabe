//! Embeds the PHP files the worker loads, listed from the directories that hold them.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "the file generates Rust source, which rustc reads as text"
)]

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (dir, list) in [
        ("php/stubs", "stub-files.rs"),
        ("php/runtime", "runtime-files.rs"),
        ("php/guards", "guard-files.rs"),
    ] {
        write_embedded_files(&manifest_dir.join(dir), &out_dir.join(list));
    }
}

/// Writes the initializer of one of the lists of PHP files the worker loads (`STUB_FILES`,
/// `RUNTIME_FILES`, `GUARD_FILES`): one `include_str!` per file under `dir`, keyed by its path
/// relative to `dir`, so the list is whatever the directory holds.
fn write_embedded_files(dir: &std::path::Path, list: &std::path::Path) {
    println!("cargo::rerun-if-changed={}", dir.display());

    let mut code = String::from("&[\n");
    for file in find_files(dir) {
        code += &format!(
            "    (php_str(b{:?}), include_str!({:?}).as_bytes()),\n",
            relative_path(dir, &file),
            file.display().to_string()
        );
    }
    code += "]\n";
    std::fs::write(list, code).unwrap();
}

/// The files under `root`, sorted by path, with dot files left out.
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
        if path
            .file_name()
            .unwrap()
            .display()
            .to_string()
            .starts_with('.')
        {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn relative_path(root: &std::path::Path, file: &std::path::Path) -> String {
    file.strip_prefix(root).unwrap().display().to_string()
}
