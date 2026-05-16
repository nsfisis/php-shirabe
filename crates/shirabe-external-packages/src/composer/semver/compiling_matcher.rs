use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CompilingMatcher;

impl CompilingMatcher {
    pub fn r#match(constraint: &dyn std::any::Any, package: &dyn std::any::Any) -> bool {
        todo!()
    }

    pub fn matches(constraint: &dyn std::any::Any, operator: &str, version: &str) -> bool {
        todo!()
    }

    pub fn match_(constraint: &dyn std::any::Any, operator: &str, version: &str) -> bool {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
