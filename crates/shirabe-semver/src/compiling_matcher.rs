//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php

use crate::constraint::AnyConstraint;
use crate::constraint::SimpleConstraint;
use indexmap::IndexMap;
use std::sync::Mutex;
use std::sync::OnceLock;

static COMPILED_CHECKER_CACHE: OnceLock<
    Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>>,
> = OnceLock::new();
static RESULT_CACHE: OnceLock<Mutex<IndexMap<String, bool>>> = OnceLock::new();

// Rust does not support eval(), so the compiled checker path is always disabled.
// The COMPILED_CHECKER_CACHE is retained structurally but never populated.
static TRANS_OP_INT: &[(i64, &str)] = &[
    (SimpleConstraint::OP_EQ, SimpleConstraint::STR_OP_EQ),
    (SimpleConstraint::OP_LT, SimpleConstraint::STR_OP_LT),
    (SimpleConstraint::OP_LE, SimpleConstraint::STR_OP_LE),
    (SimpleConstraint::OP_GT, SimpleConstraint::STR_OP_GT),
    (SimpleConstraint::OP_GE, SimpleConstraint::STR_OP_GE),
    (SimpleConstraint::OP_NE, SimpleConstraint::STR_OP_NE),
];

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

    pub fn r#match(constraint: &AnyConstraint, operator: i64, version: String) -> bool {
        let result_cache_key = format!("{}{};{}", operator, constraint, version);

        {
            let cache = Self::result_cache().lock().unwrap();
            if let Some(&result) = cache.get(&result_cache_key) {
                return result;
            }
        }

        let trans_op = TRANS_OP_INT
            .iter()
            .find(|(op, _)| *op == operator)
            .map(|(_, s)| *s)
            .expect("unknown operator");
        let result =
            constraint.matches(&SimpleConstraint::new(trans_op.to_string(), version, None).into());

        Self::result_cache()
            .lock()
            .unwrap()
            .insert(result_cache_key, result);
        result
    }
}
