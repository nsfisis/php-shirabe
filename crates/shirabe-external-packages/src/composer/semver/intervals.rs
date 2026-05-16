use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Intervals;

impl Intervals {
    pub fn is_subset_of(
        constraint_a: &dyn std::any::Any,
        constraint_b: &dyn std::any::Any,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn compact_constraint(constraint: &dyn std::any::Any) -> Box<dyn std::any::Any> {
        todo!()
    }

    pub fn compact(constraint: &dyn std::any::Any) -> Box<dyn std::any::Any> {
        todo!()
    }

    pub fn get(constraint: &dyn std::any::Any) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
