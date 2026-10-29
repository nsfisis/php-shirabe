//! ref: composer/src/Composer/Repository/AdvisoryProviderInterface.php

use crate::advisory::AnySecurityAdvisory;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct SecurityAdvisoryResult {
    pub names_found: Vec<BString>,
    pub advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
}

pub trait AdvisoryProviderInterface {
    fn has_security_advisories(&mut self) -> anyhow::Result<bool>;

    fn get_security_advisories(
        &mut self,
        package_constraint_map: IndexMap<BString, AnyConstraint>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult>;
}
