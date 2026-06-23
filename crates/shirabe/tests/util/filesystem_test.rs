//! ref: composer/tests/Composer/Test/Util/FilesystemTest.php

// These exercise Filesystem path helpers and on-disk operations (sizes, copy, symlinks and
// junctions over a temp tree). The on-disk tests build their temp tree with tempfile::TempDir in
// place of TestCase::getUniqueTmpDirectory, which is not ported.
use shirabe::util::filesystem::Filesystem;
use shirabe::util::platform::Platform;
use shirabe_php_shim::{
    dirname, file_exists, file_put_contents, is_dir, is_file, mkdir, symlink, touch,
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
            expected,
            fs.find_shortest_path(a, b, directory, prefer_relative)
        );
    }
}

#[test]
fn test_remove_directory_php() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_str().unwrap().to_string();

    mkdir(&format!("{working_dir}/level1/level2"), 0o777, true);
    file_put_contents(
        &format!("{working_dir}/level1/level2/hello.txt"),
        b"hello world",
    );

    let mut fs = Filesystem::new(None);
    assert!(fs.remove_directory_php(&working_dir).unwrap());
    assert!(!file_exists(format!(
        "{working_dir}/level1/level2/hello.txt"
    )));
}

#[test]
fn test_file_size() {
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = format!("{}/composer_test_file", unique_tmp.path().to_str().unwrap());

    file_put_contents(&test_file, b"Hello");

    let fs = Filesystem::new(None);
    assert!(fs.size(&test_file).unwrap() >= 5);
}

#[test]
fn test_directory_size() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_str().unwrap().to_string();

    mkdir(&working_dir, 0o777, true);
    file_put_contents(&format!("{working_dir}/file1.txt"), b"Hello");
    file_put_contents(&format!("{working_dir}/file2.txt"), b"World");

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
        assert_eq!(expected, fs.normalize_path(actual));
    }
}

#[test]
fn test_unlink_symlinked_directory() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let basepath = working_dir.path().to_str().unwrap().to_string();
    let symlinked = format!("{basepath}/linked");
    mkdir(&format!("{basepath}/real"), 0o777, true);
    touch(&format!("{basepath}/real/FILE"));

    let result = symlink(&format!("{basepath}/real"), &symlinked);

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
    let working_dir = working_dir.path().to_str().unwrap().to_string();

    mkdir(&format!("{working_dir}/real"), 0o777, true);
    touch(&format!("{working_dir}/real/FILE"));
    let symlinked = format!("{working_dir}/linked");
    let symlinked_trailing_slash = format!("{symlinked}/");

    let result = symlink(&format!("{working_dir}/real"), &symlinked);

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
    let working_dir = working_dir.path().to_str().unwrap().to_string();

    mkdir(&format!("{working_dir}/real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    // Non-Windows systems do not support this and will return false on all tests, and an exception
    // on creation.
    if !Platform::is_windows() {
        assert!(!fs.is_junction(&working_dir));
        assert!(!fs.remove_junction(&working_dir).unwrap());

        let target = format!("{working_dir}/real/../real/nesting");
        let junction = format!("{working_dir}/junction");
        let err = fs.junction(&target, &junction).unwrap_err();
        assert!(
            err.to_string()
                .contains("not available on non-Windows platform")
        );
        return;
    }

    let target = format!("{working_dir}/real/../real/nesting");
    let junction = format!("{working_dir}/junction");

    // Create and detect junction
    fs.junction(&target, &junction).unwrap();
    assert!(fs.is_junction(&junction), "{junction}: is a junction");
    assert!(!fs.is_junction(&target), "{target}: is not a junction");
    assert!(
        fs.is_junction(&format!("{target}/../../junction")),
        "{target}/../../junction: is a junction"
    );
    assert!(
        !fs.is_junction(&format!("{junction}/../real")),
        "{junction}/../real: is not a junction"
    );
    assert!(
        fs.is_junction(&format!("{junction}/../junction")),
        "{junction}/../junction: is a junction"
    );

    // Remove junction
    assert!(is_dir(&junction), "{junction} is a directory");
    assert!(
        fs.remove_junction(&junction).unwrap(),
        "{junction} has been removed"
    );
    assert!(!is_dir(&junction), "{junction} is not a directory");
}

#[test]
fn test_override_junctions() {
    if !Platform::is_windows() {
        // Only runs on windows.
        return;
    }

    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_str().unwrap().to_string();

    mkdir(&format!("{working_dir}/real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    let old_target = format!("{working_dir}/real/nesting/testing");
    let target = format!("{working_dir}/real/../real/nesting");
    let junction = format!("{working_dir}/junction");

    // Override non-broken junction
    fs.junction(&old_target, &junction).unwrap();
    fs.junction(&target, &junction).unwrap();

    assert!(fs.is_junction(&junction), "{junction}: is a junction");
    assert!(
        fs.is_junction(&format!("{target}/../../junction")),
        "{target}/../../junction: is a junction"
    );

    // Remove junction
    assert!(
        fs.remove_junction(&junction).unwrap(),
        "{junction} has been removed"
    );

    // Override broken junction
    fs.junction(&old_target, &junction).unwrap();
    fs.remove_directory(&old_target).unwrap();
    fs.junction(&target, &junction).unwrap();

    assert!(fs.is_junction(&junction), "{junction}: is a junction");
    assert!(
        fs.is_junction(&format!("{target}/../../junction")),
        "{target}/../../junction: is a junction"
    );
}

#[test]
fn test_copy() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_str().unwrap().to_string();
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = format!("{}/composer_test_file", unique_tmp.path().to_str().unwrap());

    mkdir(&format!("{working_dir}/foo/bar"), 0o777, true);
    mkdir(&format!("{working_dir}/foo/baz"), 0o777, true);
    file_put_contents(&format!("{working_dir}/foo/foo.file"), b"foo");
    file_put_contents(&format!("{working_dir}/foo/bar/foobar.file"), b"foobar");
    file_put_contents(&format!("{working_dir}/foo/baz/foobaz.file"), b"foobaz");
    file_put_contents(&test_file, b"testfile");

    let mut fs = Filesystem::new(None);

    let result1 = fs
        .copy(
            &format!("{working_dir}/foo"),
            &format!("{working_dir}/foop"),
        )
        .unwrap();
    assert!(result1, "Copying directory failed.");
    assert!(
        is_dir(format!("{working_dir}/foop")),
        "Not a directory: {working_dir}/foop"
    );
    assert!(
        is_dir(format!("{working_dir}/foop/bar")),
        "Not a directory: {working_dir}/foop/bar"
    );
    assert!(
        is_dir(format!("{working_dir}/foop/baz")),
        "Not a directory: {working_dir}/foop/baz"
    );
    assert!(
        file_exists(format!("{working_dir}/foop/foo.file")),
        "Not a file: {working_dir}/foop/foo.file"
    );
    assert!(
        file_exists(format!("{working_dir}/foop/bar/foobar.file")),
        "Not a file: {working_dir}/foop/bar/foobar.file"
    );
    assert!(
        file_exists(format!("{working_dir}/foop/baz/foobaz.file")),
        "Not a file: {working_dir}/foop/baz/foobaz.file"
    );

    let result2 = fs
        .copy(&test_file, &format!("{working_dir}/testfile.file"))
        .unwrap();
    assert!(result2);
    assert!(file_exists(format!("{working_dir}/testfile.file")));
}

#[test]
fn test_copy_then_remove() {
    let working_dir = tempfile::TempDir::new().unwrap();
    let working_dir = working_dir.path().to_str().unwrap().to_string();
    let unique_tmp = tempfile::TempDir::new().unwrap();
    let test_file = format!("{}/composer_test_file", unique_tmp.path().to_str().unwrap());

    mkdir(&format!("{working_dir}/foo/bar"), 0o777, true);
    mkdir(&format!("{working_dir}/foo/baz"), 0o777, true);
    file_put_contents(&format!("{working_dir}/foo/foo.file"), b"foo");
    file_put_contents(&format!("{working_dir}/foo/bar/foobar.file"), b"foobar");
    file_put_contents(&format!("{working_dir}/foo/baz/foobaz.file"), b"foobaz");
    file_put_contents(&test_file, b"testfile");

    let mut fs = Filesystem::new(None);

    fs.copy_then_remove(&test_file, &format!("{working_dir}/testfile.file"))
        .unwrap();
    assert!(!file_exists(&test_file), "Still a file: {test_file}");

    fs.copy_then_remove(
        &format!("{working_dir}/foo"),
        &format!("{working_dir}/foop"),
    )
    .unwrap();
    assert!(
        !file_exists(format!("{working_dir}/foo/baz/foobaz.file")),
        "Still a file: {working_dir}/foo/baz/foobaz.file"
    );
    assert!(
        !file_exists(format!("{working_dir}/foo/bar/foobar.file")),
        "Still a file: {working_dir}/foo/bar/foobar.file"
    );
    assert!(
        !file_exists(format!("{working_dir}/foo/foo.file")),
        "Still a file: {working_dir}/foo/foo.file"
    );
    assert!(
        !is_dir(format!("{working_dir}/foo/baz")),
        "Still a directory: {working_dir}/foo/baz"
    );
    assert!(
        !is_dir(format!("{working_dir}/foo/bar")),
        "Still a directory: {working_dir}/foo/bar"
    );
    assert!(
        !is_dir(format!("{working_dir}/foo")),
        "Still a directory: {working_dir}/foo"
    );
}
