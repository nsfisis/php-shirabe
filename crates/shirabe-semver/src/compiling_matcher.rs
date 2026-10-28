//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php

use crate::constraint::{AnyConstraint, SimpleConstraint};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::CmpOp;
use std::sync::{Mutex, OnceLock};

thread_local! {
    static KEY_BUFFER: std::cell::RefCell<BString> =
        const { std::cell::RefCell::new(BString::new(Vec::new())) };
}

// Rust does not support eval(), so the compiled checker path is always disabled.
// The COMPILED_CHECKER_CACHE is retained structurally but never populated.
static COMPILED_CHECKER_CACHE: OnceLock<
    Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>>,
> = OnceLock::new();
static RESULT_CACHE: OnceLock<Mutex<IndexMap<BString, bool>>> = OnceLock::new();

pub struct CompilingMatcher;

impl CompilingMatcher {
    fn compiled_checker_cache()
    -> &'static Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>> {
        COMPILED_CHECKER_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    fn result_cache() -> &'static Mutex<IndexMap<BString, bool>> {
        RESULT_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    pub fn clear() {
        Self::result_cache().lock().unwrap().clear();
        Self::compiled_checker_cache().lock().unwrap().clear();
    }

    pub fn r#match(constraint: &AnyConstraint, operator: CmpOp, version: impl AsRef<[u8]>) -> bool {
        #[derive(Debug)]
        enum CacheResult {
            Hit(bool),
            Miss(BString),
        }
        let version = version.as_ref();

        // The key is built into a reused buffer and only copied when it has to be stored, so a
        // cache hit allocates nothing.
        let cached = KEY_BUFFER.with_borrow_mut(|key| {
            key.clear();
            // The operator constants are single digits.
            key.push(b'0' + SimpleConstraint::get_operator_constant(operator) as u8);
            constraint.append_php_string(key);
            key.push(b';');
            key.extend_from_slice(version);

            let cache = Self::result_cache().lock().unwrap();
            match cache.get(key.as_slice()) {
                Some(&result) => CacheResult::Hit(result),
                None => CacheResult::Miss(key.clone()),
            }
        });
        let key = match cached {
            CacheResult::Hit(result) => return result,
            CacheResult::Miss(key) => key,
        };

        let result =
            constraint.matches(&SimpleConstraint::new(operator.as_str(), version, None).into());

        Self::result_cache().lock().unwrap().insert(key, result);
        result
    }
}
