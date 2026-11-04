//! ref: composer/tests/Composer/Test/DefaultConfigTest.php sha1:40cf762e10f9fc3999aa68d22bb677d4e55c04ca

use shirabe::config::Config;
use shirabe_php_shim::PhpMixed;

#[test]
fn test_default_values_are_as_expected() {
    let config = Config::new(true, None);
    assert_eq!(config.get("disable-tls").unwrap(), PhpMixed::Bool(false));
}
