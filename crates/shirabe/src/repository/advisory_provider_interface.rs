//! ref: composer/src/Composer/Repository/AdvisoryProviderInterface.php

use crate::advisory::{PartialOrFullSecurityAdvisory, PartialSecurityAdvisory, SecurityAdvisory};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct SecurityAdvisoryResult {
    pub names_found: Vec<String>,
    pub advisories: IndexMap<String, Vec<PartialOrFullSecurityAdvisory>>,
}

pub trait AdvisoryProviderInterface {
    fn has_security_advisories(&mut self) -> anyhow::Result<bool>;

    fn get_security_advisories(
        &mut self,
        package_constraint_map: IndexMap<String, AnyConstraint>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult>;
}
