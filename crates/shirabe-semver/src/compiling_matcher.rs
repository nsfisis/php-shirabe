//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php

use crate::constraint::AnyConstraint;
use crate::constraint::SimpleConstraint;
use indexmap::IndexMap;
use shirabe_php_shim::CmpOp;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::OnceLock;

thread_local! {
    static KEY_BUFFER: std::cell::RefCell<String> =
        const { std::cell::RefCell::new(String::new()) };
}

// Rust does not support eval(), so the compiled checker path is always disabled.
// The COMPILED_CHECKER_CACHE is retained structurally but never populated.
static COMPILED_CHECKER_CACHE: OnceLock<
    Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>>,
> = OnceLock::new();
static RESULT_CACHE: OnceLock<Mutex<IndexMap<String, bool>>> = OnceLock::new();

pub struct CompilingMatcher;

impl CompilingMatcher {
    fn compiled_checker_cache()
    -> &'static Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>> {
        COMPILED_CHECKER_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    fn result_cache() -> &'static Mutex<IndexMap<String, bool>> {
        RESULT_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    pub fn clear() {
        Self::result_cache().lock().unwrap().clear();
        Self::compiled_checker_cache().lock().unwrap().clear();
    }

    pub fn r#match(constraint: &AnyConstraint, operator: CmpOp, version: &str) -> bool {
        #[derive(Debug)]
        enum CacheResult {
            Hit(bool),
            Miss(String),
        }

        // The key is built into a reused buffer and only copied when it has to be stored, so a
        // cache hit allocates nothing.
        let cached = KEY_BUFFER.with_borrow_mut(|key| {
            key.clear();
            let _ = write!(
                key,
                "{}{};{}",
                SimpleConstraint::get_operator_constant(operator),
                constraint,
                version
            );

            let cache = Self::result_cache().lock().unwrap();
            match cache.get(key.as_str()) {
                Some(&result) => CacheResult::Hit(result),
                None => CacheResult::Miss(key.clone()),
            }
        });
        let key = match cached {
            CacheResult::Hit(result) => return result,
            CacheResult::Miss(key) => key,
        };

        let result = constraint.matches(
            &SimpleConstraint::new(operator.to_string(), version.to_string(), None).into(),
        );

        Self::result_cache().lock().unwrap().insert(key, result);
        result
    }
}
