//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php sha1:ac42c345fc54411069b305772a4532eff4c2d6b3

use crate::constraint::{AnyConstraint, SimpleConstraint};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::CmpOp;

thread_local! {
    static KEY_BUFFER: std::cell::RefCell<BString> =
        const { std::cell::RefCell::new(BString::new(Vec::new())) };

    static RESULT_CACHE: std::cell::RefCell<IndexMap<BString, bool>> =
        std::cell::RefCell::new(IndexMap::new());
}

pub struct CompilingMatcher;

impl CompilingMatcher {
    pub fn clear() {
        RESULT_CACHE.with_borrow_mut(|cache| cache.clear());
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

            RESULT_CACHE.with_borrow(|cache| match cache.get(key.as_slice()) {
                Some(&result) => CacheResult::Hit(result),
                None => CacheResult::Miss(key.clone()),
            })
        });
        let key = match cached {
            CacheResult::Hit(result) => return result,
            CacheResult::Miss(key) => key,
        };

        // TODO(perf): Composer compiles each constraint into a PHP closure with eval() and caches
        // it per operator and constraint; the constraint is matched directly here instead.
        let result =
            constraint.matches(&SimpleConstraint::new(operator.as_str(), version, None).into());

        RESULT_CACHE.with_borrow_mut(|cache| cache.insert(key, result));
        result
    }
}
