//! ref: composer/src/Composer/Util/Silencer.php

use anyhow::Result;
use shirabe_php_shim::{
    E_DEPRECATED, E_NOTICE, E_USER_DEPRECATED, E_USER_NOTICE, E_USER_WARNING, E_WARNING,
    error_reporting,
};
use std::sync::Mutex;

static STACK: Mutex<Vec<i64>> = Mutex::new(Vec::new());

pub struct Silencer;

impl Silencer {
    pub fn suppress(mask: Option<i64>) -> i64 {
        let mask = mask.unwrap_or(
            E_WARNING
                | E_NOTICE
                | E_USER_WARNING
                | E_USER_NOTICE
                | E_DEPRECATED
                | E_USER_DEPRECATED,
        );
        let old = error_reporting(None);
        STACK.lock().unwrap().push(old);
        error_reporting(Some(old & !mask));
        old
    }

    pub fn restore() {
        let mut stack = STACK.lock().unwrap();
        if !stack.is_empty() {
            let level = stack.pop().unwrap();
            drop(stack);
            error_reporting(Some(level));
        }
    }

    pub fn call<F, T>(callable: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        Self::suppress(None);
        match callable() {
            Ok(result) => {
                Self::restore();
                Ok(result)
            }
            Err(e) => {
                Self::restore();
                Err(e)
            }
        }
    }
}
