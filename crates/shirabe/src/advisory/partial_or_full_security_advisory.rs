use crate::advisory::PartialSecurityAdvisory;
use crate::advisory::SecurityAdvisory;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug, Clone)]
pub enum PartialOrFullSecurityAdvisory {
    Partial(PartialSecurityAdvisory),
    Full(SecurityAdvisory),
}

impl PartialOrFullSecurityAdvisory {
    pub fn advisory_id(&self) -> &str {
        match self {
            PartialOrFullSecurityAdvisory::Partial(p) => &p.advisory_id,
            PartialOrFullSecurityAdvisory::Full(s) => s.advisory_id(),
        }
    }

    pub fn affected_versions(&self) -> &AnyConstraint {
        match self {
            PartialOrFullSecurityAdvisory::Partial(p) => &p.affected_versions,
            PartialOrFullSecurityAdvisory::Full(s) => s.affected_versions(),
        }
    }
}
