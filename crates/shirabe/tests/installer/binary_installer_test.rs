//! ref: composer/tests/Composer/Test/Installer/BinaryInstallerTest.php

/// Creates the root/vendor/bin temp directories and a mocked IO. The temp-dir
/// helpers (`getUniqueTmpDirectory`/`ensureDirectoryExistsAndClear`) and the IO
/// mock are not available here, so this remains a stub.
fn set_up() {
    todo!()
}

/// Removes the root dir created by `set_up`, which is itself a stub.
fn tear_down() {
    todo!()
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// This installs a PHP binary and then executes it via ProcessExecutor, asserting the
// program's output. It needs a real PHP runtime, the binary-proxy generation, and a
// mocked Package's getBinaries(), none of which are available here.
#[test]
#[ignore = "installs and executes a PHP binary via ProcessExecutor (needs a real PHP runtime and binary proxies) and mocks a Package"]
fn test_install_and_exec_binary_with_full_compat() {
    todo!()
}
