//! ref: composer/tests/Composer/Test/Config/JsonConfigSourceTest.php

// JsonConfigSource edits composer.json through JsonManipulator, whose text-rewriting
// operations reach addcslashes (todo!()) in the php-shim.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "JsonConfigSource uses JsonManipulator, which reaches addcslashes (todo!()) in the php-shim"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_add_repository);
stub!(test_add_repository_as_list);
stub!(test_add_repository_with_options);
stub!(test_remove_repository);
stub!(test_add_packagist_repository_with_false_value);
stub!(test_remove_packagist);
stub!(test_add_link);
stub!(test_remove_link);
