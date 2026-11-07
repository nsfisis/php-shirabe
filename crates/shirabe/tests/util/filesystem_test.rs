//! ref: composer/tests/Composer/Test/Util/FilesystemTest.php sha1:78faaa186186548f6c2fb9e64ddaa92c01fb6058

// These exercise Filesystem path helpers and on-disk operations (sizes, copy, symlinks and
// junctions over a temp tree). The on-disk tests build their temp tree with tempfile::TempDir in
// place of TestCase::getUniqueTmpDirectory, which is not ported.
use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, ByteSlice};
use shirabe::util::filesystem::Filesystem;
use shirabe::util::platform::Platform;
use shirabe_php_shim::{
    error_message, file_exists, file_put_contents, is_dir, mkdir, path_from_bytes, path_to_bytes,
    symlink, touch,
};

// PHP's setUp/tearDown build workingDir/testFile under TestCase::getUniqueTmpDirectory; the
// on-disk tests below instead create their own tempfile::TempDir inline, so no shared fixture
// helper is needed.

/// providePathCouplesAsCode: (a, b, directory, expected, static, prefer_relative)
fn provide_path_couples_as_code() -> Vec<(
    &'static BStr,
    &'static BStr,
    bool,
    &'static BStr,
    bool,
    bool,
)> {
    vec![
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar"),
            false,
            BStr::new("__FILE__"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/baz"),
            false,
            BStr::new("__DIR__.'/baz'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/foo/vendor/acme/bin/run"),
            false,
            BStr::new("dirname(__DIR__).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/bar/bin/run"),
            false,
            BStr::new("'/bar/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("c:/vendor/acme/bin/run"),
            false,
            BStr::new("dirname(__DIR__).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("c:/vendor/acme/bin/run"),
            false,
            BStr::new("dirname(__DIR__).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("D:/vendor/acme/bin/run"),
            false,
            BStr::new("'D:/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("d:/vendor/acme/bin/run"),
            false,
            BStr::new("'D:/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar"),
            true,
            BStr::new("__DIR__"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar/"),
            BStr::new("/foo/bar"),
            true,
            BStr::new("__DIR__"),
            false,
            false,
        ),
        (
            BStr::new("/foo"),
            BStr::new("/baz"),
            true,
            BStr::new("dirname(__DIR__).'/baz'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/baz"),
            true,
            BStr::new("dirname(__DIR__).'/baz'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/foo/vendor/acme/bin/run"),
            true,
            BStr::new("dirname(dirname(__DIR__)).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/bar/bin/run"),
            true,
            BStr::new("'/bar/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("/app/vendor/foo/bar"),
            BStr::new("/lib"),
            true,
            BStr::new("dirname(dirname(dirname(dirname(__DIR__)))).'/lib'"),
            false,
            true,
        ),
        (
            BStr::new("/bin/run"),
            BStr::new("/bin/run"),
            true,
            BStr::new("__DIR__"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("C:\\bin/run"),
            true,
            BStr::new("__DIR__"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("c:/vendor/acme/bin/run"),
            true,
            BStr::new("dirname(dirname(__DIR__)).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("c:/vendor/acme/bin/run"),
            true,
            BStr::new("dirname(dirname(__DIR__)).'/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("d:/vendor/acme/bin/run"),
            true,
            BStr::new("'D:/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("d:/vendor/acme/bin/run"),
            true,
            BStr::new("'D:/vendor/acme/bin/run'"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp/test"),
            BStr::new("C:\\Temp"),
            true,
            BStr::new("dirname(__DIR__)"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("C:\\Temp\\test"),
            true,
            BStr::new("__DIR__ . '/test'"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test"),
            BStr::new("/tmp"),
            true,
            BStr::new("dirname(__DIR__)"),
            false,
            false,
        ),
        (
            BStr::new("/tmp"),
            BStr::new("/tmp/test"),
            true,
            BStr::new("__DIR__ . '/test'"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("c:\\Temp\\test"),
            true,
            BStr::new("__DIR__ . '/test'"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/./"),
            BStr::new("/tmp/test/"),
            true,
            BStr::new("__DIR__"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/../vendor"),
            BStr::new("/tmp/test"),
            true,
            BStr::new("dirname(__DIR__).'/test'"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/.././vendor"),
            BStr::new("/tmp/test"),
            true,
            BStr::new("dirname(__DIR__).'/test'"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("c:\\Temp\\..\\..\\test"),
            true,
            BStr::new("dirname(__DIR__).'/test'"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp/../.."),
            BStr::new("d:\\Temp\\..\\..\\test"),
            true,
            BStr::new("'D:/test'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar_vendor"),
            true,
            BStr::new("dirname(__DIR__).'/bar_vendor'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar"),
            true,
            BStr::new("dirname(__DIR__).'/bar'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar/src"),
            true,
            BStr::new("dirname(__DIR__).'/bar/src'"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor/src2"),
            BStr::new("/foo/bar/src/lib"),
            true,
            BStr::new("dirname(dirname(__DIR__)).'/bar/src/lib'"),
            false,
            false,
        ),
        // static use case
        (
            BStr::new("/tmp/test/../vendor"),
            BStr::new("/tmp/test"),
            true,
            BStr::new("__DIR__ . '/..'.'/test'"),
            true,
            false,
        ),
        (
            BStr::new("/tmp/test/.././vendor"),
            BStr::new("/tmp/test"),
            true,
            BStr::new("__DIR__ . '/..'.'/test'"),
            true,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("c:\\Temp\\..\\..\\test"),
            true,
            BStr::new("__DIR__ . '/..'.'/test'"),
            true,
            false,
        ),
        (
            BStr::new("C:/Temp/../.."),
            BStr::new("d:\\Temp\\..\\..\\test"),
            true,
            BStr::new("'D:/test'"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar_vendor"),
            true,
            BStr::new("__DIR__ . '/..'.'/bar_vendor'"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar"),
            true,
            BStr::new("__DIR__ . '/..'.'/bar'"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar/src"),
            true,
            BStr::new("__DIR__ . '/..'.'/bar/src'"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor/src2"),
            BStr::new("/foo/bar/src/lib"),
            true,
            BStr::new("__DIR__ . '/../..'.'/bar/src/lib'"),
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
            fs.find_shortest_path_code(
                path_from_bytes(a),
                path_from_bytes(b),
                directory,
                static_code,
                prefer_relative
            )
            .unwrap()
        );
    }
}

/// providePathCouples: (a, b, expected, directory, prefer_relative)
fn provide_path_couples() -> Vec<(&'static BStr, &'static BStr, &'static BStr, bool, bool)> {
    vec![
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar"),
            BStr::new("./bar"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/baz"),
            BStr::new("./baz"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar/"),
            BStr::new("/foo/baz"),
            BStr::new("./baz"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar"),
            BStr::new("./"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/baz"),
            BStr::new("../baz"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar/"),
            BStr::new("/foo/baz"),
            BStr::new("../baz"),
            true,
            false,
        ),
        (
            BStr::new("C:/foo/bar/"),
            BStr::new("c:/foo/baz"),
            BStr::new("../baz"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/foo/vendor/acme/bin/run"),
            BStr::new("../vendor/acme/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/bar/bin/run"),
            BStr::new("/bar/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("/foo/bin/run"),
            BStr::new("/bar/bin/run"),
            BStr::new("/bar/bin/run"),
            true,
            false,
        ),
        (
            BStr::new("c:/foo/bin/run"),
            BStr::new("d:/bar/bin/run"),
            BStr::new("D:/bar/bin/run"),
            true,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("c:/vendor/acme/bin/run"),
            BStr::new("../vendor/acme/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("c:/vendor/acme/bin/run"),
            BStr::new("../vendor/acme/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("c:/bin/run"),
            BStr::new("d:/vendor/acme/bin/run"),
            BStr::new("D:/vendor/acme/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("c:\\bin\\run"),
            BStr::new("d:/vendor/acme/bin/run"),
            BStr::new("D:/vendor/acme/bin/run"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp/test"),
            BStr::new("C:\\Temp"),
            BStr::new("./"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test"),
            BStr::new("/tmp"),
            BStr::new("./"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp/test/sub"),
            BStr::new("C:\\Temp"),
            BStr::new("../"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/sub"),
            BStr::new("/tmp"),
            BStr::new("../"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/sub"),
            BStr::new("/tmp"),
            BStr::new("../../"),
            true,
            false,
        ),
        (
            BStr::new("c:/tmp/test/sub"),
            BStr::new("c:/tmp"),
            BStr::new("../../"),
            true,
            false,
        ),
        (
            BStr::new("/tmp"),
            BStr::new("/tmp/test"),
            BStr::new("test"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("C:\\Temp\\test"),
            BStr::new("test"),
            false,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("c:\\Temp\\test"),
            BStr::new("test"),
            false,
            false,
        ),
        (
            BStr::new("/tmp/test/./"),
            BStr::new("/tmp/test"),
            BStr::new("./"),
            true,
            false,
        ),
        (
            BStr::new("/tmp/test/../vendor"),
            BStr::new("/tmp/test"),
            BStr::new("../test"),
            true,
            false,
        ),
        (
            BStr::new("/tmp/test/.././vendor"),
            BStr::new("/tmp/test"),
            BStr::new("../test"),
            true,
            false,
        ),
        (
            BStr::new("C:/Temp"),
            BStr::new("c:\\Temp\\..\\..\\test"),
            BStr::new("../test"),
            true,
            false,
        ),
        (
            BStr::new("C:/Temp/../.."),
            BStr::new("c:\\Temp\\..\\..\\test"),
            BStr::new("./test"),
            true,
            false,
        ),
        (
            BStr::new("C:/Temp/../.."),
            BStr::new("D:\\Temp\\..\\..\\test"),
            BStr::new("D:/test"),
            true,
            false,
        ),
        (
            BStr::new("/app/vendor/foo/bar"),
            BStr::new("/lib"),
            BStr::new("../../../../lib"),
            true,
            true,
        ),
        (
            BStr::new("/tmp"),
            BStr::new("/tmp/../../test"),
            BStr::new("../test"),
            true,
            false,
        ),
        (
            BStr::new("/tmp"),
            BStr::new("/test"),
            BStr::new("../test"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar"),
            BStr::new("/foo/bar_vendor"),
            BStr::new("../bar_vendor"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar"),
            BStr::new("../bar"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor"),
            BStr::new("/foo/bar/src"),
            BStr::new("../bar/src"),
            true,
            false,
        ),
        (
            BStr::new("/foo/bar_vendor/src2"),
            BStr::new("/foo/bar/src/lib"),
            BStr::new("../../bar/src/lib"),
            true,
            false,
        ),
        (
            BStr::new("C:/"),
            BStr::new("C:/foo/bar/"),
            BStr::new("foo/bar"),
            true,
            false,
        ),
    ]
}

#[test]
fn test_find_shortest_path() {
    let fs = Filesystem::new(None);
    for (a, b, expected, directory, prefer_relative) in provide_path_couples() {
        assert_eq!(
            bstr::BStr::new(expected),
            bstr::BStr::new(path_to_bytes(
                &fs.find_shortest_path(
                    path_from_bytes(a),
                    path_from_bytes(b),
                    directory,
                    prefer_relative
                )
                .unwrap()
            ))
        );
    }
}

#[test]
fn test_remove_directory_php() {
    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("level1/level2"), 0o777, true);
    file_put_contents(working_dir.join("level1/level2/hello.txt"), b"hello world");

    let mut fs = Filesystem::new(None);
    assert!(fs.remove_directory_php(&working_dir).unwrap());
    assert!(!file_exists(working_dir.join("level1/level2/hello.txt")));
}

#[test]
fn test_file_size() {
    let unique_tmp = get_unique_tmp_directory();
    let test_file = unique_tmp.path().join("composer_test_file");

    file_put_contents(&test_file, b"Hello");

    let fs = Filesystem::new(None);
    assert!(fs.size(&test_file).unwrap() >= 5);
}

#[test]
fn test_directory_size() {
    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(&working_dir, 0o777, true);
    file_put_contents(working_dir.join("file1.txt"), b"Hello");
    file_put_contents(working_dir.join("file2.txt"), b"World");

    let fs = Filesystem::new(None);
    assert!(fs.size(&working_dir).unwrap() >= 10);
}

/// provideNormalizedPaths: (expected, actual)
fn provide_normalized_paths() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        (BStr::new("../foo"), BStr::new("../foo")),
        (BStr::new("C:/foo/bar"), BStr::new("c:/foo//bar")),
        (BStr::new("C:/foo/bar"), BStr::new("C:/foo/./bar")),
        (BStr::new("C:/foo/bar"), BStr::new("C://foo//bar")),
        (BStr::new("C:/foo/bar"), BStr::new("C:///foo//bar")),
        (BStr::new("C:/bar"), BStr::new("C:/foo/../bar")),
        (BStr::new("/bar"), BStr::new("/foo/../bar/")),
        (
            BStr::new("phar://C:/Foo"),
            BStr::new("phar://c:/Foo/Bar/.."),
        ),
        (
            BStr::new("phar://C:/Foo"),
            BStr::new("phar://c:///Foo/Bar/.."),
        ),
        (
            BStr::new("phar://C:/"),
            BStr::new("phar://c:/Foo/Bar/../../../.."),
        ),
        (BStr::new("/"), BStr::new("/Foo/Bar/../../../..")),
        (BStr::new("/"), BStr::new("/")),
        (BStr::new("/"), BStr::new("//")),
        (BStr::new("/"), BStr::new("///")),
        (BStr::new("/Foo"), BStr::new("///Foo")),
        (BStr::new("C:/"), BStr::new("c:\\")),
        (BStr::new("../src"), BStr::new("Foo/Bar/../../../src")),
        (BStr::new("C:../b"), BStr::new("c:.\\..\\a\\..\\b")),
        (BStr::new("phar://C:../Foo"), BStr::new("phar://c:../Foo")),
        (BStr::new("//foo/bar"), BStr::new("\\\\foo\\bar")),
    ]
}

#[test]
fn test_normalize_path() {
    let fs = Filesystem::new(None);
    for (expected, actual) in provide_normalized_paths() {
        assert_eq!(
            bstr::BStr::new(expected),
            bstr::BStr::new(path_to_bytes(&fs.normalize_path(path_from_bytes(actual))))
        );
    }
}

#[test]
fn test_unlink_symlinked_directory() {
    let working_dir = get_unique_tmp_directory();
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
    let working_dir = get_unique_tmp_directory();
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
    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    // Non-Windows systems do not support this and will return false on all tests, and an exception
    // on creation.
    if !Platform::is_windows() {
        assert!(!fs.is_junction(&working_dir));
        assert!(!fs.remove_junction(&working_dir).unwrap());

        let target = working_dir.join("real/../real/nesting");
        let junction = working_dir.join("junction");
        let err = fs.junction(&target, &junction).unwrap_err();
        assert!(error_message(&err).contains_str("not available on non-Windows platform"));
        return;
    }

    let target = working_dir.join("real/../real/nesting");
    let junction = working_dir.join("junction");

    // Create and detect junction
    fs.junction(&target, &junction).unwrap();
    assert!(
        fs.is_junction(&junction),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        !fs.is_junction(&target),
        "{}: is not a junction",
        target.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction")),
        "{}/../../junction: is a junction",
        target.display()
    );
    assert!(
        !fs.is_junction(junction.join("../real")),
        "{}/../real: is not a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(junction.join("../junction")),
        "{}/../junction: is a junction",
        junction.display()
    );

    // Remove junction
    assert!(is_dir(&junction), "{} is a directory", junction.display());
    assert!(
        fs.remove_junction(&junction).unwrap(),
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

    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();

    mkdir(working_dir.join("real/nesting/testing"), 0o777, true);
    let mut fs = Filesystem::new(None);

    let old_target = working_dir.join("real/nesting/testing");
    let target = working_dir.join("real/../real/nesting");
    let junction = working_dir.join("junction");

    // Override non-broken junction
    fs.junction(&old_target, &junction).unwrap();
    fs.junction(&target, &junction).unwrap();

    assert!(
        fs.is_junction(&junction),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction")),
        "{}/../../junction: is a junction",
        target.display()
    );

    // Remove junction
    assert!(
        fs.remove_junction(&junction).unwrap(),
        "{} has been removed",
        junction.display()
    );

    // Override broken junction
    fs.junction(&old_target, &junction).unwrap();
    fs.remove_directory(&old_target).unwrap();
    fs.junction(&target, &junction).unwrap();

    assert!(
        fs.is_junction(&junction),
        "{}: is a junction",
        junction.display()
    );
    assert!(
        fs.is_junction(target.join("../../junction")),
        "{}/../../junction: is a junction",
        target.display()
    );
}

#[test]
fn test_copy() {
    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();
    let unique_tmp = get_unique_tmp_directory();
    let test_file = unique_tmp.path().join("composer_test_file");

    mkdir(working_dir.join("foo/bar"), 0o777, true);
    mkdir(working_dir.join("foo/baz"), 0o777, true);
    file_put_contents(working_dir.join("foo/foo.file"), b"foo");
    file_put_contents(working_dir.join("foo/bar/foobar.file"), b"foobar");
    file_put_contents(working_dir.join("foo/baz/foobaz.file"), b"foobaz");
    file_put_contents(&test_file, b"testfile");

    let mut fs = Filesystem::new(None);

    let result1 = fs
        .copy(working_dir.join("foo"), working_dir.join("foop"))
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
        .copy(&test_file, working_dir.join("testfile.file"))
        .unwrap();
    assert!(result2);
    assert!(file_exists(working_dir.join("testfile.file")));
}

#[test]
fn test_copy_then_remove() {
    let working_dir = get_unique_tmp_directory();
    let working_dir = working_dir.path().to_path_buf();
    let unique_tmp = get_unique_tmp_directory();
    let test_file = unique_tmp.path().join("composer_test_file");

    mkdir(working_dir.join("foo/bar"), 0o777, true);
    mkdir(working_dir.join("foo/baz"), 0o777, true);
    file_put_contents(working_dir.join("foo/foo.file"), b"foo");
    file_put_contents(working_dir.join("foo/bar/foobar.file"), b"foobar");
    file_put_contents(working_dir.join("foo/baz/foobaz.file"), b"foobaz");
    file_put_contents(&test_file, b"testfile");

    let mut fs = Filesystem::new(None);

    fs.copy_then_remove(&test_file, working_dir.join("testfile.file"))
        .unwrap();
    assert!(
        !file_exists(&test_file),
        "Still a file: {}",
        test_file.display()
    );

    fs.copy_then_remove(working_dir.join("foo"), working_dir.join("foop"))
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
