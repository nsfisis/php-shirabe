//! ref: composer/tests/Composer/Test/Package/Archiver/ArchivableFilesFinderTest.php sha1:9d96f4e20720a26a3662e8c5d191999cc82eda1d

use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::archiver::ArchivableFilesFinder;
use shirabe::util::Filesystem;
use shirabe_php_shim::{
    PhpMixed, ZipArchive, bformat, dirname, file_put_contents, path_from_bytes, path_to_bytes,
    preg_quote, preg_replace,
};
use shirabe_symfony_process::Process;
use tempfile::TempDir;

struct SetUp {
    _sources_dir: TempDir,
    sources: std::path::PathBuf,
    fs: Filesystem,
}

fn set_up() -> SetUp {
    let mut fs = Filesystem::new(None);

    let sources_dir = get_unique_tmp_directory();
    let sources = fs.normalize_path(sources_dir.path());

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
        let path = std::path::Path::new(&sources).join(relative_path);
        fs.ensure_directory_exists(path_from_bytes(&dirname(path_to_bytes(&path))))
            .unwrap();
        file_put_contents(&path, b"");
    }

    SetUp {
        _sources_dir: sources_dir,
        sources: sources.clone(),
        fs,
    }
}

fn get_archivable_files(set_up: &SetUp, finder: ArchivableFilesFinder) -> Vec<BString> {
    let mut files: Vec<BString> = vec![];
    for file in finder {
        if !file.is_dir() {
            let real_path = file.canonicalize().unwrap();
            files.push(preg_replace(
                bformat!(
                    "#^{}#",
                    preg_quote(path_to_bytes(&set_up.sources), Some(b'#'))
                ),
                "",
                path_to_bytes(&set_up.fs.normalize_path(&real_path)),
            ));
        }
    }

    files.sort();

    files
}

fn assert_archivable_files(
    set_up: &SetUp,
    finder: ArchivableFilesFinder,
    expected_files: &[impl AsRef<[u8]>],
) {
    let actual_files = get_archivable_files(set_up, finder);

    let expected_files: Vec<BString> = expected_files
        .iter()
        .map(|s| BString::from(s.as_ref()))
        .collect();
    assert_eq!(expected_files, actual_files);
}

#[test]
fn test_manual_excludes() {
    let set_up = set_up();

    let excludes: Vec<BString> = vec![
        "prefixB.foo".into(),
        "!/prefixB.foo".into(),
        "/prefixA.foo".into(),
        "prefixC.*".into(),
        "!*/*/*/prefixC.foo".into(),
        ".*".into(),
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

/// ref: ArchivableFilesFinderTest::getArchivedFiles
///
/// PHP reads the produced zip via `PharData` + `RecursiveIteratorIterator`, which yields only
/// leaf files; `ZipArchive` entry enumeration (skipping directory entries) is the faithful
/// equivalent. Each entry is rebuilt into the `phar://.../archive.zip/archive/...` virtual path
/// PHP would stringify, then the `archive` prefix is stripped.
fn get_archived_files(set_up: &SetUp, command: &BStr) -> Vec<BString> {
    let mut process = Process::from_shell_commandline(
        command,
        Some(std::path::Path::new(&set_up.sources)),
        None,
        PhpMixed::Bool(false),
        Some(60.0),
    )
    .unwrap();
    process.run(None, IndexMap::new()).unwrap();

    let zip_path = std::path::Path::new(&set_up.sources).join("archive.zip");
    let mut archive = ZipArchive::new();
    archive.open(&zip_path, 0).unwrap();

    let prefix = bformat!(
        "#^phar://{}/archive\\.zip/archive#",
        preg_quote(path_to_bytes(&set_up.sources), Some(b'#'))
    );
    let mut files: Vec<BString> = vec![];
    for index in 0..archive.count() {
        let name = archive.get_name_index(index);
        if name.ends_with(b"/") {
            continue;
        }
        let virtual_path = bformat!("phar://{}/archive.zip/{name}", &set_up.sources);
        files.push(preg_replace(
            &prefix,
            "",
            path_to_bytes(&set_up.fs.normalize_path(path_from_bytes(&virtual_path))),
        ));
    }

    let _ = std::fs::remove_file(&zip_path);

    files
}

/// PHP's `skipIfNotExecutable('git')`.
fn git_is_executable() -> bool {
    Process::from_shell_commandline(
        "git --version",
        None,
        None,
        PhpMixed::Bool(false),
        Some(60.0),
    )
    .and_then(|mut p| p.run(None, IndexMap::new()))
    .map(|code| code == 0)
    .unwrap_or(false)
}

#[test]
fn test_git_excludes() {
    if !git_is_executable() {
        return;
    }

    let set_up = set_up();

    file_put_contents(
        std::path::Path::new(&set_up.sources).join(".gitattributes"),
        [
            "",
            "# gitattributes rules with comments and blank lines",
            "prefixB.foo export-ignore",
            "/prefixA.foo export-ignore",
            "prefixC.* export-ignore",
            "",
            "prefixE.foo export-ignore",
            "# and more",
            "# comments",
            "",
            "/prefixE.foo -export-ignore",
            "/prefixD.foo export-ignore",
            "prefixF.* export-ignore",
            "/*/*/prefixF.foo -export-ignore",
            "",
            "refixD.foo export-ignore",
            "/C export-ignore",
            "D/prefixA export-ignore",
            "E export-ignore",
            "F/ export-ignore",
            "G/* export-ignore",
            "H/** export-ignore",
            "J/ export-ignore",
            "parameters.yml export-ignore",
            "\\!important!.txt export-ignore",
            "\\#* export-ignore",
        ]
        .join("\n")
        .as_bytes(),
    );

    let finder = ArchivableFilesFinder::new(&set_up.sources, vec![], false).unwrap();

    let expected = get_archived_files(
        &set_up,
        BStr::new(
            "git init && \
         git config user.email \"you@example.com\" && \
         git config user.name \"Your Name\" && \
         git config commit.gpgsign false && \
         git add .git* && \
         git commit -m \"ignore rules\" && \
         git add . && \
         git commit -m \"init\" && \
         git archive --format=zip --prefix=archive/ -o archive.zip HEAD",
        ),
    );
    assert_archivable_files(&set_up, finder, &expected);
}

#[test]
fn test_skip_excludes() {
    let set_up = set_up();

    let excludes: Vec<BString> = vec!["prefixB.foo".into()];

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
