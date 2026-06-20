//! ref: composer/tests/Composer/Test/Installer/BinaryInstallerTest.php

// This installs a PHP binary and then executes it via ProcessExecutor, asserting the
// program's output. It needs a real PHP runtime, the binary-proxy generation, and a
// mocked Package's getBinaries(), none of which are available here.
#[test]
#[ignore = "installs and executes a PHP binary via ProcessExecutor (needs a real PHP runtime and binary proxies) and mocks a Package"]
fn test_install_and_exec_binary_with_full_compat() {
    todo!()
}
