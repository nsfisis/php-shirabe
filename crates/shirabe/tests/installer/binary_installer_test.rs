//! ref: composer/tests/Composer/Test/Installer/BinaryInstallerTest.php

use crate::test_case::get_package;
use base64::Engine;
use shirabe::installer::BinaryInstaller;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::util::Filesystem;
use shirabe::util::ProcessExecutor;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use tempfile::TempDir;

/// Mirror of setUp(): builds temp root/vendor/bin dirs plus a mocked IO. PHP uses a
/// PHPUnit IOInterface mock with no expectations; a NullIO is the closest analogue.
struct SetUp {
    root: TempDir,
    vendor_dir: String,
    bin_dir: String,
    io: Rc<RefCell<dyn IOInterface>>,
    fs: Filesystem,
}

fn set_up() -> SetUp {
    let fs = Filesystem::new(None);

    let root = TempDir::new().unwrap();
    let root_dir = fs::canonicalize(root.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let vendor_dir = format!("{}/vendor", root_dir);
    fs::create_dir_all(&vendor_dir).unwrap();

    let bin_dir = format!("{}/bin", root_dir);
    fs::create_dir_all(&bin_dir).unwrap();

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));

    SetUp {
        root,
        vendor_dir,
        bin_dir,
        io,
        fs,
    }
}

fn tear_down(setup: &mut SetUp) {
    let root = setup.root.path().to_path_buf();
    setup.fs.remove_directory(&root).ok();
}

/// ref: BinaryInstallerTest::executableBinaryProvider
fn executable_binary_provider() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "simple php file",
            b"<?php\n\necho 'success '.$_SERVER['argv'][1];".to_vec(),
        ),
        (
            "php file with shebang",
            b"#!/usr/bin/env php\n<?php\n\necho 'success '.$_SERVER['argv'][1];".to_vec(),
        ),
        (
            "phar file",
            base64::engine::general_purpose::STANDARD
                .decode("IyEvdXNyL2Jpbi9lbnYgcGhwCjw/cGhwCgpQaGFyOjptYXBQaGFyKCd0ZXN0LnBoYXInKTsKCnJlcXVpcmUgJ3BoYXI6Ly90ZXN0LnBoYXIvcnVuLnBocCc7CgpfX0hBTFRfQ09NUElMRVIoKTsgPz4NCj4AAAABAAAAEQAAAAEACQAAAHRlc3QucGhhcgAAAAAHAAAAcnVuLnBocCoAAADb9n9hKgAAAMUDDWGkAQAAAAAAADw/cGhwIGVjaG8gInN1Y2Nlc3MgIi4kX1NFUlZFUlsiYXJndiJdWzFdO1SOC0IE3+UN0yzrHIwyspp9slhmAgAAAEdCTUI=")
                .unwrap(),
        ),
        (
            "shebang with strict types declare",
            b"#!/usr/bin/env php\n<?php declare(strict_types=1);\n\necho 'success '.$_SERVER['argv'][1];".to_vec(),
        ),
    ]
}

/// ref: BinaryInstallerTest::testInstallAndExecBinaryWithFullCompat
fn run_install_and_exec_binary_with_full_compat(contents: &[u8]) {
    let mut setup = set_up();

    // PHP mocks Package::getBinaries() to return ['binary']; here a real package is
    // configured via the __set_binaries test helper.
    let package = get_package("foo/bar", "1.0.0");
    package.__set_binaries(vec!["binary".to_string()]);

    let pkg_dir = format!("{}/foo/bar", setup.vendor_dir);
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(format!("{}/binary", pkg_dir), contents).unwrap();

    let mut installer = BinaryInstaller::new(
        setup.io.clone(),
        setup.bin_dir.clone(),
        "full".to_string(),
        Some(Rc::new(RefCell::new(Filesystem::new(None)))),
        None,
    );
    installer.install_binaries(package, &pkg_dir, true);

    let mut proc = ProcessExecutor::new(None);
    let mut output = String::new();
    proc.execute(format!("{}/binary arg", setup.bin_dir), &mut output, None)
        .unwrap();
    assert_eq!("", proc.get_error_output());
    assert_eq!("success arg", output);

    tear_down(&mut setup);
}

#[test]
fn test_install_and_exec_binary_with_full_compat_simple_php_file() {
    run_install_and_exec_binary_with_full_compat(&executable_binary_provider()[0].1);
}

#[test]
fn test_install_and_exec_binary_with_full_compat_php_file_with_shebang() {
    run_install_and_exec_binary_with_full_compat(&executable_binary_provider()[1].1);
}

#[test]
fn test_install_and_exec_binary_with_full_compat_phar_file() {
    run_install_and_exec_binary_with_full_compat(&executable_binary_provider()[2].1);
}

#[test]
fn test_install_and_exec_binary_with_full_compat_shebang_with_strict_types_declare() {
    run_install_and_exec_binary_with_full_compat(&executable_binary_provider()[3].1);
}
