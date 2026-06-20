//! ref: composer/tests/Composer/Test/Util/ProcessExecutorTest.php

// These run real subprocesses (capturing output/stderr/timeout) and assert ProcessExecutor's
// password hiding, line splitting and argument escaping; the subprocess execution and mocked
// IO are not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (runs subprocesses / mocks IO; argument-escaping and split helpers included)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_execute_captures_output);
stub!(test_execute_outputs_if_not_captured);
stub!(test_use_io_is_not_null_and_if_not_captured);
stub!(test_execute_captures_stderr);
stub!(test_timeout);
stub!(test_hide_passwords);
stub!(test_doesnt_hide_ports);
stub!(test_split_lines);
stub!(test_console_io_does_not_format_symfony_console_style);
stub!(test_execute_async_cancel);
stub!(test_escape_argument);
