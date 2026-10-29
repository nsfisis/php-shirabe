//! ref: composer/tests/Composer/Test/Util/FilesystemTest.php

// These exercise Filesystem path helpers and on-disk operations (sizes, copy, symlinks and
// junctions over a temp tree). The on-disk tests build their temp tree with tempfile::TempDir in
// place of TestCase::getUniqueTmpDirectory, which is not ported.
use shirabe::util::filesystem::Filesystem;
use shirabe::util::platform::Platform;
use shirabe_php_shim::{
    file_exists, file_put_contents, is_dir, mkdir, path_to_bytes, symlink, touch,
};

// PHP's setUp/tearDown build workingDir/testFile under TestCase::getUniqueTmpDirectory; the
// on-disk tests below instead create their own tempfile::TempDir inline, so no shared fixture
// helper is needed.

/// providePathCouplesAsCode: (a, b, directory, expected, static, prefer_relative)
fn provide_path_couples_as_code()
-> Vec<(&'static str, &'static str, bool, &'static str, bool, bool)> {
    vec![
        ("/foo/bar", "/foo/bar", false, "__FILE__", false, false),
        (
            "/foo/bar",
            "/foo/baz",
            false,
            "__DIR__.'/baz'",
            false,
            false,
        ),
        (
            "/foo/bin/run",
            "/foo/vendor/acme/bin/run",
            false,
            "dirname(__DIR__).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "/foo/bin/run",
            "/bar/bin/run",
            false,
            "'/bar/bin/run'",
            false,
            false,
        ),
        (
            "c:/bin/run",
            "c:/vendor/acme/bin/run",
            false,
            "dirname(__DIR__).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "c:/vendor/acme/bin/run",
            false,
            "dirname(__DIR__).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:/bin/run",
            "D:/vendor/acme/bin/run",
            false,
            "'D:/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "d:/vendor/acme/bin/run",
            false,
            "'D:/vendor/acme/bin/run'",
            false,
            false,
        ),
        ("/foo/bar", "/foo/bar", true, "__DIR__", false, false),
        ("/foo/bar/", "/foo/bar", true, "__DIR__", false, false),
        (
            "/foo",
            "/baz",
            true,
            "dirname(__DIR__).'/baz'",
            false,
            false,
        ),
        (
            "/foo/bar",
            "/foo/baz",
            true,
            "dirname(__DIR__).'/baz'",
            false,
            false,
        ),
        (
            "/foo/bin/run",
            "/foo/vendor/acme/bin/run",
            true,
            "dirname(dirname(__DIR__)).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "/foo/bin/run",
            "/bar/bin/run",
            true,
            "'/bar/bin/run'",
            false,
            false,
        ),
        (
            "/app/vendor/foo/bar",
            "/lib",
            true,
            "dirname(dirname(dirname(dirname(__DIR__)))).'/lib'",
            false,
            true,
        ),
        ("/bin/run", "/bin/run", true, "__DIR__", false, false),
        ("c:/bin/run", "C:\\bin/run", true, "__DIR__", false, false),
        (
            "c:/bin/run",
            "c:/vendor/acme/bin/run",
            true,
            "dirname(dirname(__DIR__)).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "c:/vendor/acme/bin/run",
            true,
            "dirname(dirname(__DIR__)).'/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:/bin/run",
            "d:/vendor/acme/bin/run",
            true,
            "'D:/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "d:/vendor/acme/bin/run",
            true,
            "'D:/vendor/acme/bin/run'",
            false,
            false,
        ),
        (
            "C:/Temp/test",
            "C:\\Temp",
            true,
            "dirname(__DIR__)",
            false,
            false,
        ),
        (
            "C:/Temp",
            "C:\\Temp\\test",
            true,
            "__DIR__ . '/test'",
            false,
            false,
        ),
        ("/tmp/test", "/tmp", true, "dirname(__DIR__)", false, false),
        ("/tmp", "/tmp/test", true, "__DIR__ . '/test'", false, false),
        (
            "C:/Temp",
            "c:\\Temp\\test",
            true,
            "__DIR__ . '/test'",
            false,
            false,
        ),
        ("/tmp/test/./", "/tmp/test/", true, "__DIR__", false, false),
        (
            "/tmp/test/../vendor",
            "/tmp/test",
            true,
            "dirname(__DIR__).'/test'",
            false,
            false,
        ),
        (
            "/tmp/test/.././vendor",
            "/tmp/test",
            true,
            "dirname(__DIR__).'/test'",
            false,
            false,
        ),
        (
            "C:/Temp",
            "c:\\Temp\\..\\..\\test",
            true,
            "dirname(__DIR__).'/test'",
            false,
            false,
        ),
        (
            "C:/Temp/../..",
            "d:\\Temp\\..\\..\\test",
            true,
            "'D:/test'",
            false,
            false,
        ),
        (
            "/foo/bar",
            "/foo/bar_vendor",
            true,
            "dirname(__DIR__).'/bar_vendor'",
            false,
            false,
        ),
        (
            "/foo/bar_vendor",
            "/foo/bar",
            true,
            "dirname(__DIR__).'/bar'",
            false,
            false,
        ),
        (
            "/foo/bar_vendor",
            "/foo/bar/src",
            true,
            "dirname(__DIR__).'/bar/src'",
            false,
            false,
        ),
        (
            "/foo/bar_vendor/src2",
            "/foo/bar/src/lib",
            true,
            "dirname(dirname(__DIR__)).'/bar/src/lib'",
            false,
            false,
        ),
        // static use case
        (
            "/tmp/test/../vendor",
            "/tmp/test",
            true,
            "__DIR__ . '/..'.'/test'",
            true,
            false,
        ),
        (
            "/tmp/test/.././vendor",
            "/tmp/test",
            true,
            "__DIR__ . '/..'.'/test'",
            true,
            false,
        ),
        (
            "C:/Temp",
            "c:\\Temp\\..\\..\\test",
            true,
            "__DIR__ . '/..'.'/test'",
            true,
            false,
        ),
        (
            "C:/Temp/../..",
            "d:\\Temp\\..\\..\\test",
            true,
            "'D:/test'",
            true,
            false,
        ),
        (
            "/foo/bar",
            "/foo/bar_vendor",
            true,
            "__DIR__ . '/..'.'/bar_vendor'",
            true,
            false,
        ),
        (
            "/foo/bar_vendor",
            "/foo/bar",
            true,
            "__DIR__ . '/..'.'/bar'",
            true,
            false,
        ),
        (
            "/foo/bar_vendor",
            "/foo/bar/src",
            true,
            "__DIR__ . '/..'.'/bar/src'",
            true,
            false,
        ),
        (
            "/foo/bar_vendor/src2",
            "/foo/bar/src/lib",
            true,
            "__DIR__ . '/../..'.'/bar/src/lib'",
            true,
            false,
        ),
    ]
}

#[test]
fn test_find_shortest_path_code() {
    let fs = Filesystem::new(None);
    for (a, b, directory, expected, static_code, prefer_relative) in provide_path_couples_as_code()
    {
        assert_eq!(
            expected,
            fs.find_shortest_path_code(a, b, directory, static_code, prefer_relative)
                .unwrap()
        );
    }
}

/// providePathCouples: (a, b, expected, directory, prefer_relative)
fn provide_path_couples() -> Vec<(&'static str, &'static str, &'static str, bool, bool)> {
    vec![
        ("/foo/bar", "/foo/bar", "./bar", false, false),
        ("/foo/bar", "/foo/baz", "./baz", false, false),
        ("/foo/bar/", "/foo/baz", "./baz", false, false),
        ("/foo/bar", "/foo/bar", "./", true, false),
        ("/foo/bar", "/foo/baz", "../baz", true, false),
        ("/foo/bar/", "/foo/baz", "../baz", true, false),
        ("C:/foo/bar/", "c:/foo/baz", "../baz", true, false),
        (
            "/foo/bin/run",
            "/foo/vendor/acme/bin/run",
            "../vendor/acme/bin/run",
            false,
            false,
        ),
        ("/foo/bin/run", "/bar/bin/run", "/bar/bin/run", false, false),
        ("/foo/bin/run", "/bar/bin/run", "/bar/bin/run", true, false),
        (
            "c:/foo/bin/run",
            "d:/bar/bin/run",
            "D:/bar/bin/run",
            true,
            false,
        ),
        (
            "c:/bin/run",
            "c:/vendor/acme/bin/run",
            "../vendor/acme/bin/run",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "c:/vendor/acme/bin/run",
            "../vendor/acme/bin/run",
            false,
            false,
        ),
        (
            "c:/bin/run",
            "d:/vendor/acme/bin/run",
            "D:/vendor/acme/bin/run",
            false,
            false,
        ),
        (
            "c:\\bin\\run",
            "d:/vendor/acme/bin/run",
            "D:/vendor/acme/bin/run",
            false,
            false,
        ),
        ("C:/Temp/test", "C:\\Temp", "./", false, false),
        ("/tmp/test", "/tmp", "./", false, false),
        ("C:/Temp/test/sub", "C:\\Temp", "../", false, false),
        ("/tmp/test/sub", "/tmp", "../", false, false),
        ("/tmp/test/sub", "/tmp", "../../", true, false),
        ("c:/tmp/test/sub", "c:/tmp", "../../", true, false),
        ("/tmp", "/tmp/test", "test", false, false),
        ("C:/Temp", "C:\\Temp\\test", "test", false, false),
        ("C:/Temp", "c:\\Temp\\test", "test", false, false),
        ("/tmp/test/./", "/tmp/test", "./", true, false),
        ("/tmp/test/../vendor", "/tmp/test", "../test", true, false),
        ("/tmp/test/.././vendor", "/tmp/test", "../test", true, false),
        ("C:/Temp", "c:\\Temp\\..\\..\\test", "../test", true, false),
        (
            "C:/Temp/../..",
            "c:\\Temp\\..\\..\\test",
            "./test",
            true,
            false,
        ),
        (
            "C:/Temp/../..",
            "D:\\Temp\\..\\..\\test",
            "D:/test",
            true,
            false,
        ),
        ("/app/vendor/foo/bar", "/lib", "../../../../lib", true, true),
        ("/tmp", "/tmp/../../test", "../test", true, false),
        ("/tmp", "/test", "../test", true, false),
        ("/foo/bar", "/foo/bar_vendor", "../bar_vendor", true, false),
        ("/foo/bar_vendor", "/foo/bar", "../bar", true, false),
        ("/foo/bar_vendor", "/foo/bar/src", "../bar/src", true, false),
        (
            "/foo/bar_vendor/src2",
            "/foo/bar/src/lib",
            "../../bar/src/lib",
            true,
            false,
        ),
        ("C:/", "C:/foo/bar/", "foo/bar", true, false),
    ]
}

#[test]
fn test_find_shortest_path() {
    let fs = Filesystem::new(None);
    for (a, b, expected, directory, prefer_relative) in provide_path_couples() {
        assert_eq!(
            bstr::BStr::new(expected),
            bstr::BStr::new(path_to_bytes(
                &fs.find_shortest_path(a, b, directory, prefer_relative)
                    .unwrap()
            ))
        );
    }
}

#[test]
fn test_remove_directory_php() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("level1/level2"), 0o777, true);
    file_put_contents(
        working_dir
            .join("level1/level2/hello.txt")
            .to_str()
            .unwrap(),
        b"hello world",
    );

    let mut fs = Filesystem::new(None);
    assert!(
        fs.remove_directory_php(working_dir.to_str().unwrap())
            .unwrap()
    );
    assert!(!file_exists(working_dir.join("level1/level2/hello.txt")));
}

#[test]
fn test_file_size() {
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = unique_tmp.path().join("composer_test_file");

    file_put_contents(test_file.to_str().unwrap(), b"Hello");

    let fs = Filesystem::new(None);
    assert!(fs.size(&test_file).unwrap() >= 5);
}

#[test]
fn test_directory_size() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(&working_dir, 0o777, true);
    file_put_contents(working_dir.join("file1.txt").to_str().unwrap(), b"Hello");
    file_put_contents(working_dir.join("file2.txt").to_str().unwrap(), b"World");

    let fs = Filesystem::new(None);
    assert!(fs.size(&working_dir).unwrap() >= 10);
}

/// provideNormalizedPaths: (expected, actual)
fn provide_normalized_paths() -> Vec<(&'static str, &'static str)> {
    vec![
        ("../foo", "../foo"),
        ("C:/foo/bar", "c:/foo//bar"),
        ("C:/foo/bar", "C:/foo/./bar"),
        ("C:/foo/bar", "C://foo//bar"),
        ("C:/foo/bar", "C:///foo//bar"),
        ("C:/bar", "C:/foo/../bar"),
        ("/bar", "/foo/../bar/"),
        ("phar://C:/Foo", "phar://c:/Foo/Bar/.."),
        ("phar://C:/Foo", "phar://c:///Foo/Bar/.."),
        ("phar://C:/", "phar://c:/Foo/Bar/../../../.."),
        ("/", "/Foo/Bar/../../../.."),
        ("/", "/"),
        ("/", "//"),
        ("/", "///"),
        ("/Foo", "///Foo"),
        ("C:/", "c:\\"),
        ("../src", "Foo/Bar/../../../src"),
        ("C:../b", "c:.\\..\\a\\..\\b"),
        ("phar://C:../Foo", "phar://c:../Foo"),
        ("//foo/bar", "\\\\foo\\bar"),
    ]
}

#[test]
fn test_normalize_path() {
    let fs = Filesystem::new(None);
    for (expected, actual) in provide_normalized_paths() {
        assert_eq!(
            bstr::BStr::new(expected),
            bstr::BStr::new(path_to_bytes(&fs.normalize_path(actual)))
        );
    }
}

#[test]
fn test_unlink_symlinked_directory() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let basepath = working_dir.path().to_path_buf();
    let symlinked = basepath.join("linked");
    mkdir(basepath.join("real"), 0o777, true);
    touch(basepath.join("real/FILE"));

    let result = symlink(basepath.join("real"), &symlinked).is_ok();

    if !result {
        // Symbolic links for directories not supported on this platform.
        return;
    }

    if !is_dir(&symlinked) {
        panic!("Precondition assertion failed (is_dir is false on symbolic link to directory).");
    }

    let fs = Filesystem::new(None);
    let result = fs.unlink(&symlinked).unwrap();
    assert!(result);
    assert!(!file_exists(&symlinked));
}

#[test]
fn test_remove_symlinked_directory_with_trailing_slash() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("real"), 0o777, true);
    touch(working_dir.join("real/FILE"));
    let symlinked = working_dir.join("linked");
    let symlinked_trailing_slash = symlinked.join("");

    let result = symlink(working_dir.join("real"), &symlinked).is_ok();

    if !result {
        // Symbolic links for directories not supported on this platform.
        return;
    }

    if !is_dir(&symlinked) {
        panic!("Precondition assertion failed (is_dir is false on symbolic link to directory).");
    }

    if !is_dir(&symlinked_trailing_slash) {
        panic!("Precondition assertion failed (is_dir false w trailing slash).");
    }

    let mut fs = Filesystem::new(None);

    let result = fs.remove_directory(&symlinked_trailing_slash).unwrap();
    assert!(result);
    assert!(!file_exists(&symlinked_trailing_slash));
    assert!(!file_exists(&symlinked));
}

#[test]
fn test_junctions() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    // Non-Windows systems do not support this and will return false on all tests, and an exception
    // on creation.
    if !Platform::is_windows() {
        assert!(!fs.is_junction(working_dir.to_str().unwrap()));
        assert!(!fs.remove_junction(working_dir.to_str().unwrap()).unwrap());

        let target = working_dir.join("real/../real/nesting");
        let junction = working_dir.join("junction");
        let err = fs
            .junction(target.to_str().unwrap(), junction.to_str().unwrap())
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("not available on non-Windows platform")
        );
        return;
    }

    let target = working_dir.join("real/../real/nesting");
    let junction = working_dir.join("junction");

    // Create and detect junction
    fs.junction(target.to_str().unwrap(), junction.to_str().unwrap())
        .unwrap();
    assert!(
        fs.is_junction(junction.to_str().unwrap()),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        !fs.is_junction(target.to_str().unwrap()),
        "{}: is not a junction",
        target.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction").to_str().unwrap()),
        "{}/../../junction: is a junction",
        target.display()
    );
    assert!(
        !fs.is_junction(junction.join("../real").to_str().unwrap()),
        "{}/../real: is not a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(junction.join("../junction").to_str().unwrap()),
        "{}/../junction: is a junction",
        junction.display()
    );

    // Remove junction
    assert!(is_dir(&junction), "{} is a directory", junction.display());
    assert!(
        fs.remove_junction(junction.to_str().unwrap()).unwrap(),
        "{} has been removed",
        junction.display()
    );
    assert!(
        !is_dir(&junction),
        "{} is not a directory",
        junction.display()
    );
}

#[test]
fn test_override_junctions() {
    if !Platform::is_windows() {
        // Only runs on windows.
        return;
    }

    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    let old_target = working_dir.join("real/nesting/testing");
    let target = working_dir.join("real/../real/nesting");
    let junction = working_dir.join("junction");

    // Override non-broken junction
    fs.junction(old_target.to_str().unwrap(), junction.to_str().unwrap())
        .unwrap();
    fs.junction(target.to_str().unwrap(), junction.to_str().unwrap())
        .unwrap();

    assert!(
        fs.is_junction(junction.to_str().unwrap()),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction").to_str().unwrap()),
        "{}/../../junction: is a junction",
        target.display()
    );

    // Remove junction
    assert!(
        fs.remove_junction(junction.to_str().unwrap()).unwrap(),
        "{} has been removed",
        junction.display()
    );

    // Override broken junction
    fs.junction(old_target.to_str().unwrap(), junction.to_str().unwrap())
        .unwrap();
    fs.remove_directory(&old_target).unwrap();
    fs.junction(target.to_str().unwrap(), junction.to_str().unwrap())
        .unwrap();

    assert!(
        fs.is_junction(junction.to_str().unwrap()),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction").to_str().unwrap()),
        "{}/../../junction: is a junction",
        target.display()
    );
}

#[test]
fn test_copy() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = unique_tmp.path().join("composer_test_file");

    mkdir(working_dir.join("foo/bar"), 0o777, true);
    mkdir(working_dir.join("foo/baz"), 0o777, true);
    file_put_contents(working_dir.join("foo/foo.file").to_str().unwrap(), b"foo");
    file_put_contents(
        working_dir.join("foo/bar/foobar.file").to_str().unwrap(),
        b"foobar",
    );
    file_put_contents(
        working_dir.join("foo/baz/foobaz.file").to_str().unwrap(),
        b"foobaz",
    );
    file_put_contents(test_file.to_str().unwrap(), b"testfile");

    let mut fs = Filesystem::new(None);

    let result1 = fs
        .copy(
            working_dir.join("foo").to_str().unwrap(),
            working_dir.join("foop").to_str().unwrap(),
        )
        .unwrap();
    assert!(result1, "Copying directory failed.");
    assert!(
        is_dir(working_dir.join("foop")),
        "Not a directory: {}/foop",
        working_dir.display()
    );
    assert!(
        is_dir(working_dir.join("foop/bar")),
        "Not a directory: {}/foop/bar",
        working_dir.display()
    );
    assert!(
        is_dir(working_dir.join("foop/baz")),
        "Not a directory: {}/foop/baz",
        working_dir.display()
    );
    assert!(
        file_exists(working_dir.join("foop/foo.file")),
        "Not a file: {}/foop/foo.file",
        working_dir.display()
    );
    assert!(
        file_exists(working_dir.join("foop/bar/foobar.file")),
        "Not a file: {}/foop/bar/foobar.file",
        working_dir.display()
    );
    assert!(
        file_exists(working_dir.join("foop/baz/foobaz.file")),
        "Not a file: {}/foop/baz/foobaz.file",
        working_dir.display()
    );

    let result2 = fs
        .copy(
            test_file.to_str().unwrap(),
            working_dir.join("testfile.file").to_str().unwrap(),
        )
        .unwrap();
    assert!(result2);
    assert!(file_exists(working_dir.join("testfile.file")));
}

#[test]
fn test_copy_then_remove() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_path_buf();
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = unique_tmp.path().join("composer_test_file");

    mkdir(working_dir.join("foo/bar"), 0o777, true);
    mkdir(working_dir.join("foo/baz"), 0o777, true);
    file_put_contents(working_dir.join("foo/foo.file").to_str().unwrap(), b"foo");
    file_put_contents(
        working_dir.join("foo/bar/foobar.file").to_str().unwrap(),
        b"foobar",
    );
    file_put_contents(
        working_dir.join("foo/baz/foobaz.file").to_str().unwrap(),
        b"foobaz",
    );
    file_put_contents(test_file.to_str().unwrap(), b"testfile");

    let mut fs = Filesystem::new(None);

    fs.copy_then_remove(
        test_file.to_str().unwrap(),
        working_dir.join("testfile.file").to_str().unwrap(),
    )
    .unwrap();
    assert!(
        !file_exists(&test_file),
        "Still a file: {}",
        test_file.display()
    );

    fs.copy_then_remove(
        working_dir.join("foo").to_str().unwrap(),
        working_dir.join("foop").to_str().unwrap(),
    )
    .unwrap();
    assert!(
        !file_exists(working_dir.join("foo/baz/foobaz.file")),
        "Still a file: {}/foo/baz/foobaz.file",
        working_dir.display()
    );
    assert!(
        !file_exists(working_dir.join("foo/bar/foobar.file")),
        "Still a file: {}/foo/bar/foobar.file",
        working_dir.display()
    );
    assert!(
        !file_exists(working_dir.join("foo/foo.file")),
        "Still a file: {}/foo/foo.file",
        working_dir.display()
    );
    assert!(
        !is_dir(working_dir.join("foo/baz")),
        "Still a directory: {}/foo/baz",
        working_dir.display()
    );
    assert!(
        !is_dir(working_dir.join("foo/bar")),
        "Still a directory: {}/foo/bar",
        working_dir.display()
    );
    assert!(
        !is_dir(working_dir.join("foo")),
        "Still a directory: {}/foo",
        working_dir.display()
    );
}
