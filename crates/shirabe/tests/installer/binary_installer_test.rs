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

// This installs a PHP binary (via NullIO + Package::__set_binaries + a real tempdir) and then
// executes it through ProcessExecutor, asserting its output. Setup and binary-proxy generation
// all work; the remaining blocker is real subprocess I/O: ProcessExecutor -> symfony Process
// drives its pipe reads through `stream_select`/`stream_set_blocking`, both of which are still
// `todo!()` in shirabe-php-shim (they require select(2)/fcntl(2) over the child pipe fds, which
// the shim does not yet expose). Un-ignore once that pipe-reading layer is implemented.
#[test]
#[ignore = "ProcessExecutor cannot read a real child's output yet: shirabe_php_shim::{stream_select, stream_set_blocking} are todo!() (need select(2)/fcntl(2) over child pipe fds)"]
fn test_install_and_exec_binary_with_full_compat() {
    todo!()
}
