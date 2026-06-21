//! ref: composer/tests/Composer/Test/Installer/SuggestedPackagesReporterTest.php

/// Builds an IO mock and a SuggestedPackagesReporter over it. The IO mock
/// (`getIOMock`) is not available here, so this remains a stub.
fn set_up() {
    todo!()
}

// These construct a SuggestedPackagesReporter with a mocked IO and assert its accumulated
// suggestions and formatted output; mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO to drive SuggestedPackagesReporter output; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_constructor);
stub!(test_get_packages_empty_by_default);
stub!(test_get_packages);
stub!(test_add_package_appends);
stub!(test_add_suggestions_from_package);
stub!(test_output);
stub!(test_output_with_no_suggestion_reason);
stub!(test_output_ignores_formatting);
stub!(test_output_multiple_packages);
stub!(test_output_skip_installed_packages);
stub!(test_output_not_getting_installed_packages_when_no_suggestions);
