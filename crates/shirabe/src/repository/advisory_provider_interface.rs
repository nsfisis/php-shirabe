//! ref: composer/src/Composer/Repository/AdvisoryProviderInterface.php

use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::advisory::security_advisory::SecurityAdvisory;
use indexmap::IndexMap;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug)]
pub enum PartialOrSecurityAdvisory {
    Partial(PartialSecurityAdvisory),
    Full(SecurityAdvisory),
}

#[derive(Debug)]
pub struct SecurityAdvisoryResult {
    pub names_found: Vec<String>,
    pub advisories: IndexMap<String, Vec<PartialOrSecurityAdvisory>>,
}

pub trait AdvisoryProviderInterface {
    fn has_security_advisories(&self) -> bool;

    fn get_security_advisories(
        &self,
        package_constraint_map: IndexMap<String, Box<dyn ConstraintInterface>>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult>;
}
