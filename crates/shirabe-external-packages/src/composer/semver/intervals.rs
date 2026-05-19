use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug)]
pub struct Intervals;

impl Intervals {
    pub fn is_subset_of(
        _constraint_a: &dyn ConstraintInterface,
        _constraint_b: &dyn ConstraintInterface,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn compact_constraint(
        _constraint: Box<dyn ConstraintInterface>,
    ) -> Box<dyn ConstraintInterface> {
        todo!()
    }

    pub fn compact(_constraint: &dyn ConstraintInterface) -> Box<dyn ConstraintInterface> {
        todo!()
    }

    pub fn get(_constraint: &dyn ConstraintInterface) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
