use shirabe_semver::constraint::ConstraintInterface;

#[derive(Debug)]
pub struct CompilingMatcher;

impl CompilingMatcher {
    pub fn r#match(_constraint: &dyn ConstraintInterface, _package: &dyn std::any::Any) -> bool {
        todo!()
    }

    pub fn matches(_constraint: &dyn ConstraintInterface, _operator: i64, _version: &str) -> bool {
        todo!()
    }

    pub fn match_(_constraint: &dyn ConstraintInterface, _operator: i64, _version: &str) -> bool {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
