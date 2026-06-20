//! ref: composer/tests/Composer/Test/Package/Archiver/ArchiveManagerTest.php

// These drive ArchiveManager end-to-end (building tar archives via PharData, todo!()) and
// the filename-derivation helpers over packages; the archiving and fixture setup are not
// ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "ArchiveManager builds archives via PharData (todo!()) over fixtures; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_unknown_format);
stub!(test_archive_tar);
stub!(test_archive_custom_file_name);
stub!(test_get_package_filename_parts);
stub!(test_get_package_filename);
