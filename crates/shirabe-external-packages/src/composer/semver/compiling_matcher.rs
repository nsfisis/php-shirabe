#[derive(Debug)]
pub struct CompilingMatcher;

impl CompilingMatcher {
    pub fn r#match(_constraint: &dyn std::any::Any, _package: &dyn std::any::Any) -> bool {
        todo!()
    }

    pub fn matches(_constraint: &dyn std::any::Any, _operator: &str, _version: &str) -> bool {
        todo!()
    }

    pub fn match_(_constraint: &dyn std::any::Any, _operator: &str, _version: &str) -> bool {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
