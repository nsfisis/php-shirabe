//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/PlatformRequirementFilterFactoryTest.php

use bstr::BString;
use shirabe::filter::platform_requirement_filter::{
    IgnoreAllPlatformRequirementFilter, IgnoreListPlatformRequirementFilter,
    IgnoreNothingPlatformRequirementFilter, PlatformRequirementFilterFactory,
};
use shirabe_php_shim::{PhpMixed, error_message};

#[test]
fn test_from_bool_or_list() {
    // 'true creates IgnoreAllFilter'
    let filter = PlatformRequirementFilterFactory::from_bool_or_list(PhpMixed::Bool(true)).unwrap();
    assert!(
        filter
            .as_any()
            .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
            .is_some()
    );

    // 'false creates IgnoreNothingFilter'
    let filter =
        PlatformRequirementFilterFactory::from_bool_or_list(PhpMixed::Bool(false)).unwrap();
    assert!(
        filter
            .as_any()
            .downcast_ref::<IgnoreNothingPlatformRequirementFilter>()
            .is_some()
    );

    // 'list creates IgnoreListFilter'
    let filter = PlatformRequirementFilterFactory::from_bool_or_list(PhpMixed::List(vec![
        PhpMixed::string(BString::from("php")),
        PhpMixed::string(BString::from("ext-json")),
    ]))
    .unwrap();
    assert!(
        filter
            .as_any()
            .downcast_ref::<IgnoreListPlatformRequirementFilter>()
            .is_some()
    );
}

#[test]
fn test_from_bool_throws_exception_if_type_is_unknown() {
    let result = PlatformRequirementFilterFactory::from_bool_or_list(PhpMixed::Null);
    let err = result.unwrap_err();
    assert_eq!(
        "PlatformRequirementFilter: Unknown $boolOrList parameter null. Please report at https://github.com/nsfisis/php-shirabe/issues/new.",
        error_message(&err)
    );
}

#[test]
fn test_ignore_all() {
    let platform_requirement_filter = PlatformRequirementFilterFactory::ignore_all();

    assert!(
        platform_requirement_filter
            .as_any()
            .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
            .is_some()
    );
}

#[test]
fn test_ignore_nothing() {
    let platform_requirement_filter = PlatformRequirementFilterFactory::ignore_nothing();

    assert!(
        platform_requirement_filter
            .as_any()
            .downcast_ref::<IgnoreNothingPlatformRequirementFilter>()
            .is_some()
    );
}
