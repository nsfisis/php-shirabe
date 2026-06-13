//! ref: composer/vendor/composer/semver/src/CompilingMatcher.php

use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct CompilingMatcher;

impl CompilingMatcher {
    pub fn r#match(_constraint: &AnyConstraint, _package: &dyn std::any::Any) -> bool {
        todo!()
    }

    pub fn matches(_constraint: &AnyConstraint, _operator: i64, _version: &str) -> bool {
        todo!()
    }

    pub fn match_(_constraint: &AnyConstraint, _operator: i64, _version: &str) -> bool {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
