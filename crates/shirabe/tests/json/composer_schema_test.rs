//! ref: composer/tests/Composer/Test/Json/ComposerSchemaTest.php

// These validate documents against the bundled composer-schema.json via JsonFile's
// json-schema validator, which is not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "needs JsonFile schema validation against the bundled composer-schema.json (not ported)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_name_pattern);
stub!(test_version_pattern);
stub!(test_optional_abandoned_property);
stub!(test_require_types);
stub!(test_minimum_stability_values);
