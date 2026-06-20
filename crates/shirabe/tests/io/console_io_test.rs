//! ref: composer/tests/Composer/Test/IO/ConsoleIOTest.php

// These mock Symfony's InputInterface/OutputInterface/HelperSet (and QuestionHelper) to
// drive ConsoleIO; those console abstractions are not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks Symfony Input/Output/HelperSet to drive ConsoleIO; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_is_interactive);
stub!(test_write);
stub!(test_write_error);
stub!(test_write_with_multiple_line_string_when_debugging);
stub!(test_overwrite);
stub!(test_ask);
stub!(test_ask_confirmation);
stub!(test_ask_and_validate);
stub!(test_select);
stub!(test_set_and_get_authentication);
stub!(test_get_authentication_when_did_not_set);
stub!(test_has_authentication);
stub!(test_sanitize);
