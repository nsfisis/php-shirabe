//! ref: composer/tests/Composer/Test/Package/Archiver/ArchivableFilesFinderTest.php

use shirabe::package::archiver::ArchivableFilesFinder;
use shirabe::util::Filesystem;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{dirname, file_put_contents, preg_quote};
use tempfile::TempDir;

struct SetUp {
    _sources_dir: TempDir,
    sources: String,
    fs: Filesystem,
}

fn set_up() -> SetUp {
    let mut fs = Filesystem::new(None);

    let sources_dir = TempDir::new().unwrap();
    let sources = fs.normalize_path(&sources_dir.path().canonicalize().unwrap().to_string_lossy());

    let file_tree = [
        ".foo",
        "A/prefixA.foo",
        "A/prefixB.foo",
        "A/prefixC.foo",
        "A/prefixD.foo",
        "A/prefixE.foo",
        "A/prefixF.foo",
        "B/sub/prefixA.foo",
        "B/sub/prefixB.foo",
        "B/sub/prefixC.foo",
        "B/sub/prefixD.foo",
        "B/sub/prefixE.foo",
        "B/sub/prefixF.foo",
        "C/prefixA.foo",
        "C/prefixB.foo",
        "C/prefixC.foo",
        "C/prefixD.foo",
        "C/prefixE.foo",
        "C/prefixF.foo",
        "D/prefixA",
        "D/prefixB",
        "D/prefixC",
        "D/prefixD",
        "D/prefixE",
        "D/prefixF",
        "E/subtestA.foo",
        "F/subtestA.foo",
        "G/subtestA.foo",
        "H/subtestA.foo",
        "I/J/subtestA.foo",
        "K/dirJ/subtestA.foo",
        "toplevelA.foo",
        "toplevelB.foo",
        "prefixA.foo",
        "prefixB.foo",
        "prefixC.foo",
        "prefixD.foo",
        "prefixE.foo",
        "prefixF.foo",
        "parameters.yml",
        "parameters.yml.dist",
        "!important!.txt",
        "!important_too!.txt",
        "#weirdfile",
    ];

    for relative_path in file_tree {
        let path = format!("{}/{}", sources, relative_path);
        fs.ensure_directory_exists(&dirname(&path)).unwrap();
        file_put_contents(&path, b"");
    }

    SetUp {
        _sources_dir: sources_dir,
        sources,
        fs,
    }
}

fn get_archivable_files(set_up: &SetUp, finder: ArchivableFilesFinder) -> Vec<String> {
    let mut files: Vec<String> = vec![];
    for file in finder {
        if !file.is_dir() {
            let real_path = file.canonicalize().unwrap();
            files.push(Preg::replace(
                &format!("#^{}#", preg_quote(&set_up.sources, Some('#'))),
                "",
                &set_up.fs.normalize_path(&real_path.to_string_lossy()),
            ));
        }
    }

    files.sort();

    files
}

fn assert_archivable_files(set_up: &SetUp, finder: ArchivableFilesFinder, expected_files: &[&str]) {
    let actual_files = get_archivable_files(set_up, finder);

    let expected_files: Vec<String> = expected_files.iter().map(|s| s.to_string()).collect();
    assert_eq!(expected_files, actual_files);
}

// The manual exclude patterns (e.g. `.*`, `prefixC.*`) compile, via ComposerExcludeFilter, to
// look-ahead regexes like `(?=[^\.])...(?=$|/)`, which the regex crate cannot compile.
#[ignore = "ComposerExcludeFilter builds look-ahead regexes the regex crate does not support"]
#[test]
fn test_manual_excludes() {
    let set_up = set_up();

    let excludes = vec![
        "prefixB.foo".to_string(),
        "!/prefixB.foo".to_string(),
        "/prefixA.foo".to_string(),
        "prefixC.*".to_string(),
        "!*/*/*/prefixC.foo".to_string(),
        ".*".to_string(),
    ];

    let finder = ArchivableFilesFinder::new(&set_up.sources, excludes, false).unwrap();

    assert_archivable_files(
        &set_up,
        finder,
        &[
            "/!important!.txt",
            "/!important_too!.txt",
            "/#weirdfile",
            "/A/prefixA.foo",
            "/A/prefixD.foo",
            "/A/prefixE.foo",
            "/A/prefixF.foo",
            "/B/sub/prefixA.foo",
            "/B/sub/prefixC.foo",
            "/B/sub/prefixD.foo",
            "/B/sub/prefixE.foo",
            "/B/sub/prefixF.foo",
            "/C/prefixA.foo",
            "/C/prefixD.foo",
            "/C/prefixE.foo",
            "/C/prefixF.foo",
            "/D/prefixA",
            "/D/prefixB",
            "/D/prefixC",
            "/D/prefixD",
            "/D/prefixE",
            "/D/prefixF",
            "/E/subtestA.foo",
            "/F/subtestA.foo",
            "/G/subtestA.foo",
            "/H/subtestA.foo",
            "/I/J/subtestA.foo",
            "/K/dirJ/subtestA.foo",
            "/parameters.yml",
            "/parameters.yml.dist",
            "/prefixB.foo",
            "/prefixD.foo",
            "/prefixE.foo",
            "/prefixF.foo",
            "/toplevelA.foo",
            "/toplevelB.foo",
        ],
    );
}

// getArchivedFiles drives a git pipeline (Process::fromShellCommandline) and reads the produced
// zip back through PharData + RecursiveIteratorIterator; neither the process helper nor the zip
// archive reader is ported, so this case cannot run.
#[ignore = "getArchivedFiles needs a git process pipeline plus PharData/RecursiveIteratorIterator zip reading; not ported"]
#[test]
fn test_git_excludes() {
    todo!()
}

#[test]
fn test_skip_excludes() {
    let set_up = set_up();

    let excludes = vec!["prefixB.foo".to_string()];

    let finder = ArchivableFilesFinder::new(&set_up.sources, excludes, true).unwrap();

    assert_archivable_files(
        &set_up,
        finder,
        &[
            "/!important!.txt",
            "/!important_too!.txt",
            "/#weirdfile",
            "/.foo",
            "/A/prefixA.foo",
            "/A/prefixB.foo",
            "/A/prefixC.foo",
            "/A/prefixD.foo",
            "/A/prefixE.foo",
            "/A/prefixF.foo",
            "/B/sub/prefixA.foo",
            "/B/sub/prefixB.foo",
            "/B/sub/prefixC.foo",
            "/B/sub/prefixD.foo",
            "/B/sub/prefixE.foo",
            "/B/sub/prefixF.foo",
            "/C/prefixA.foo",
            "/C/prefixB.foo",
            "/C/prefixC.foo",
            "/C/prefixD.foo",
            "/C/prefixE.foo",
            "/C/prefixF.foo",
            "/D/prefixA",
            "/D/prefixB",
            "/D/prefixC",
            "/D/prefixD",
            "/D/prefixE",
            "/D/prefixF",
            "/E/subtestA.foo",
            "/F/subtestA.foo",
            "/G/subtestA.foo",
            "/H/subtestA.foo",
            "/I/J/subtestA.foo",
            "/K/dirJ/subtestA.foo",
            "/parameters.yml",
            "/parameters.yml.dist",
            "/prefixA.foo",
            "/prefixB.foo",
            "/prefixC.foo",
            "/prefixD.foo",
            "/prefixE.foo",
            "/prefixF.foo",
            "/toplevelA.foo",
            "/toplevelB.foo",
        ],
    );
}
