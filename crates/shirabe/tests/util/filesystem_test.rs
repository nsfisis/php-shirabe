//! ref: composer/tests/Composer/Test/Util/FilesystemTest.php

// These exercise Filesystem path helpers and on-disk operations (sizes, copy, symlinks and
// junctions over a temp tree). The filesystem fixtures and platform-specific symlink/junction
// behaviour are not ported.
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::{dirname, is_dir, is_file};

#[allow(dead_code)]
struct SetUp {
    fs: Filesystem,
    working_dir: String,
    test_file: String,
}

#[allow(dead_code)]
fn set_up() -> SetUp {
    let fs = Filesystem::new(None);
    // getUniqueTmpDirectory is base TestCase infrastructure that is not ported.
    let working_dir: String = todo!();
    #[allow(unreachable_code)]
    let unique_tmp: String = todo!();
    #[allow(unreachable_code)]
    let test_file: String = format!("{unique_tmp}/composer_test_file");
    #[allow(unreachable_code)]
    SetUp {
        fs,
        working_dir,
        test_file,
    }
}

#[allow(dead_code)]
fn tear_down(set_up: &mut SetUp) {
    if is_dir(&set_up.working_dir) {
        let _ = set_up.fs.remove_directory(&set_up.working_dir);
    }
    if is_file(&set_up.test_file) {
        let _ = set_up.fs.remove_directory(dirname(&set_up.test_file));
    }
}

#[allow(dead_code)]
struct TearDown {
    set_up: SetUp,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&mut self.set_up);
    }
}

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (Filesystem path helpers plus on-disk size/copy/symlink/junction fixtures)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_find_shortest_path_code);
stub!(test_find_shortest_path);
stub!(test_remove_directory_php);
stub!(test_file_size);
stub!(test_directory_size);
stub!(test_normalize_path);
stub!(test_unlink_symlinked_directory);
stub!(test_remove_symlinked_directory_with_trailing_slash);
stub!(test_junctions);
stub!(test_override_junctions);
stub!(test_copy);
stub!(test_copy_then_remove);
