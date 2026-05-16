//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php

use std::sync::Mutex;
use std::sync::OnceLock;

use indexmap::IndexMap;

use crate::constraint::constraint::Constraint;
use crate::constraint::constraint_interface::ConstraintInterface;

static COMPILED_CHECKER_CACHE: OnceLock<
    Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>>,
> = OnceLock::new();
static RESULT_CACHE: OnceLock<Mutex<IndexMap<String, bool>>> = OnceLock::new();

// Rust does not support eval(), so the compiled checker path is always disabled.
// The COMPILED_CHECKER_CACHE is retained structurally but never populated.
static TRANS_OP_INT: &[(i64, &str)] = &[
    (Constraint::OP_EQ, Constraint::STR_OP_EQ),
    (Constraint::OP_LT, Constraint::STR_OP_LT),
    (Constraint::OP_LE, Constraint::STR_OP_LE),
    (Constraint::OP_GT, Constraint::STR_OP_GT),
    (Constraint::OP_GE, Constraint::STR_OP_GE),
    (Constraint::OP_NE, Constraint::STR_OP_NE),
];

pub struct CompilingMatcher;

impl CompilingMatcher {
    fn compiled_checker_cache(
    ) -> &'static Mutex<IndexMap<String, Box<dyn Fn(String, bool) -> bool + Send + Sync>>> {
        COMPILED_CHECKER_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    fn result_cache() -> &'static Mutex<IndexMap<String, bool>> {
        RESULT_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
    }

    pub fn clear() {
        Self::result_cache().lock().unwrap().clear();
        Self::compiled_checker_cache().lock().unwrap().clear();
    }

    pub fn r#match(constraint: &dyn ConstraintInterface, operator: i64, version: String) -> bool {
        let result_cache_key =
            format!("{}{};{}", operator, constraint.__to_string(), version);

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
        let result = constraint.matches(&Constraint::new(trans_op.to_string(), version));

        Self::result_cache().lock().unwrap().insert(result_cache_key, result);
        result
    }
}
