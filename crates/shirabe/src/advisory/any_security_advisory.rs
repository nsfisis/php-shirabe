use crate::advisory::IgnoredSecurityAdvisory;
use crate::advisory::PartialSecurityAdvisory;
use crate::advisory::SecurityAdvisory;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug, Clone)]
pub enum AnySecurityAdvisory {
    Partial(PartialSecurityAdvisory),
    Full(SecurityAdvisory),
    Ignored(IgnoredSecurityAdvisory),
}

impl AnySecurityAdvisory {
    pub fn advisory_id(&self) -> &str {
        match self {
            AnySecurityAdvisory::Partial(p) => &p.advisory_id,
            AnySecurityAdvisory::Full(s) => s.advisory_id(),
            AnySecurityAdvisory::Ignored(i) => i.as_security_advisory().advisory_id(),
        }
    }

    pub fn package_name(&self) -> &str {
        match self {
            AnySecurityAdvisory::Partial(p) => &p.package_name,
            AnySecurityAdvisory::Full(s) => s.package_name(),
            AnySecurityAdvisory::Ignored(i) => i.as_security_advisory().package_name(),
        }
    }

    pub fn affected_versions(&self) -> &AnyConstraint {
        match self {
            AnySecurityAdvisory::Partial(p) => &p.affected_versions,
            AnySecurityAdvisory::Full(s) => s.affected_versions(),
            AnySecurityAdvisory::Ignored(i) => i.as_security_advisory().affected_versions(),
        }
    }

    pub fn as_security_advisory(&self) -> Option<&SecurityAdvisory> {
        match self {
            AnySecurityAdvisory::Partial(_) => None,
            AnySecurityAdvisory::Full(s) => Some(s),
            AnySecurityAdvisory::Ignored(i) => Some(i.as_security_advisory()),
        }
    }

    pub fn as_ignored(&self) -> Option<&IgnoredSecurityAdvisory> {
        match self {
            AnySecurityAdvisory::Ignored(i) => Some(i),
            _ => None,
        }
    }
}
