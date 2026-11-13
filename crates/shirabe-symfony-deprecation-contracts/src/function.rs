//! ref: composer/vendor/symfony/deprecation-contracts/function.php sha1:331f7bc29e9d280ac8a0e2c372f54975804882c0

use bstr::{BStr, BString};
use shirabe_php_shim::{E_USER_DEPRECATED, PhpTruthy as _, bformat, trigger_error};

/// Triggers a silenced deprecation notice.
///
/// Unlike upstream, this takes no printf() arguments: callers format the message themselves.
pub fn trigger_deprecation(
    package: impl AsRef<[u8]>,
    version: impl AsRef<[u8]>,
    message: impl AsRef<[u8]>,
) {
    let package = BStr::new(package.as_ref());
    let version = BStr::new(version.as_ref());
    let message = BStr::new(message.as_ref());
    let prefix = if package.is_truthy() || version.is_truthy() {
        bformat!("Since {package} {version}: ")
    } else {
        BString::default()
    };
    trigger_error(bformat!("{prefix}{message}"), E_USER_DEPRECATED);
}
