//! ref: composer/tests/Composer/Test/Util/ConfigValidatorTest.php sha1:2fe5ee17eba277a0599a543c0d555753313401d1

use shirabe::io::io_interface::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::loader::validating_array_loader::ValidatingArrayLoader;
use shirabe::util::config_validator::ConfigValidator;

fn fixture(name: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Util/Fixtures")
        .join(name)
}

fn validate(file: &std::path::Path) -> Vec<bstr::BString> {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config_validator = ConfigValidator::new(io);
    let (_, _, warnings) = config_validator.validate(
        file,
        ValidatingArrayLoader::CHECK_ALL,
        ConfigValidator::CHECK_VERSION,
    );
    warnings
}

/// Test ConfigValidator warns on commit reference
#[test]
fn test_config_validator_commit_ref_warning() {
    let warnings = validate(&fixture("composer_commit-ref.json"));

    assert!(warnings.contains(
        &bstr::BString::from("The package \"some/package\" is pointing to a commit-ref, this is bad practice and can cause unforeseen issues.")
    ));
}

#[test]
fn test_config_validator_warns_on_script_description_for_nonexistent_script() {
    let warnings = validate(&fixture("composer_scripts-descriptions.json"));

    assert!(warnings.contains(&bstr::BString::from(
        "Description for non-existent script \"phpcsxxx\" found in \"scripts-descriptions\""
    )));
}

#[test]
fn test_config_validator_warns_on_script_alias_for_nonexistent_script() {
    let warnings = validate(&fixture("composer_scripts-aliases.json"));

    assert!(warnings.contains(&bstr::BString::from(
        "Aliases for non-existent script \"phpcsxxx\" found in \"scripts-aliases\""
    )));
}

#[test]
fn test_config_validator_warns_on_unnecessary_provide_replace() {
    let warnings = validate(&fixture("composer_provide-replace-requirements.json"));

    assert!(warnings.contains(
        &bstr::BString::from("The package a/a in require is also listed in provide which satisfies the requirement. Remove it from provide if you wish to install it.")
    ));
    assert!(warnings.contains(
        &bstr::BString::from("The package b/b in require is also listed in replace which satisfies the requirement. Remove it from replace if you wish to install it.")
    ));
    assert!(warnings.contains(
        &bstr::BString::from("The package c/c in require-dev is also listed in provide which satisfies the requirement. Remove it from provide if you wish to install it.")
    ));
}
