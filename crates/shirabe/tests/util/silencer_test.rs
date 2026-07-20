//! ref: composer/tests/Composer/Test/Util/SilencerTest.php

use shirabe::util::silencer::Silencer;
use shirabe_php_shim::{
    E_USER_WARNING, RuntimeException, error_reporting, microtime, trigger_error,
};

/// Test succeeds when no warnings are emitted externally, and original level is restored.
#[test]
#[ignore = "shirabe_php_shim::trigger_error is still todo!() (PHP error subsystem not modeled)"]
fn test_silencer() {
    let before = error_reporting(None);

    // Check warnings are suppressed correctly
    Silencer::suppress(None);
    trigger_error("Test", E_USER_WARNING);
    Silencer::restore();

    // Check all parameters and return values are passed correctly in a silenced call.
    let result = Silencer::call(|| {
        trigger_error("Test", E_USER_WARNING);

        let (a, b, c) = (2, 3, 4);
        Ok(a * b * c)
    })
    .unwrap();
    assert_eq!(24, result);

    // Check the error reporting setting was restored correctly
    assert_eq!(before, error_reporting(None));
}

/// Test whether exception from silent callbacks are correctly forwarded.
#[test]
fn test_silenced_exception() {
    let verification = format!("{}", microtime());
    let err = Silencer::call(|| -> anyhow::Result<()> {
        Err(RuntimeException {
            message: verification.clone(),
            code: 0,
        }
        .into())
    })
    .unwrap_err();
    assert_eq!(verification, err.to_string());
}
