//! ref: composer/tests/Composer/Test/Util/FilesystemTest.php

// These exercise Filesystem path helpers and on-disk operations (sizes, copy, symlinks and
// junctions over a temp tree). The filesystem fixtures and platform-specific symlink/junction
// behaviour are not ported.
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
