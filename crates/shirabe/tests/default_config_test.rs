//! ref: composer/tests/Composer/Test/DefaultConfigTest.php

use shirabe::config::Config;
use shirabe_php_shim::PhpMixed;

#[test]
fn test_default_values_are_as_expected() {
    let config = Config::new(true, None);
    assert_eq!(config.get("disable-tls"), PhpMixed::Bool(false));
}
