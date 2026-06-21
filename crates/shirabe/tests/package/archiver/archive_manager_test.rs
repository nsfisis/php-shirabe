//! ref: composer/tests/Composer/Test/Package/Archiver/ArchiveManagerTest.php

/// Builds an ArchiveManager via Factory/DownloadManager/Loop and derives targetDir under a
/// unique tmp dir (testDir); none of that factory/fixture infrastructure is ported.
/// Returns (test_dir, target_dir).
#[allow(dead_code)]
fn set_up() -> (String, String) {
    todo!()
}

#[allow(dead_code)]
fn tear_down(_test_dir: &str) {
    // Removes testDir created in set_up.
    todo!()
}

#[allow(dead_code)]
struct TearDown {
    test_dir: String,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.test_dir);
    }
}

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
