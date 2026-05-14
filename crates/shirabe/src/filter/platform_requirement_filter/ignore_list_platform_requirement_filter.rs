//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreListPlatformRequirementFilter.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;
use shirabe_semver::constraint::multi_constraint::MultiConstraint;
use shirabe_semver::interval::Interval;
use shirabe_semver::intervals::Intervals;

use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::package::base_package::BasePackage;
use crate::repository::platform_repository::PlatformRepository;

#[derive(Debug)]
pub struct IgnoreListPlatformRequirementFilter {
    ignore_regex: String,
    ignore_upper_bound_regex: String,
}

impl IgnoreListPlatformRequirementFilter {
    pub fn new(req_list: Vec<String>) -> anyhow::Result<Self> {
        let mut ignore_all: Vec<String> = Vec::new();
        let mut ignore_upper_bound: Vec<String> = Vec::new();
        for req in req_list {
            if req.ends_with('+') {
                ignore_upper_bound.push(req[..req.len() - 1].to_string());
            } else {
                ignore_all.push(req);
            }
        }
        let ignore_regex = BasePackage::package_names_to_regexp(&ignore_all);
        let ignore_upper_bound_regex = BasePackage::package_names_to_regexp(&ignore_upper_bound);
        Ok(Self {
            ignore_regex,
            ignore_upper_bound_regex,
        })
    }

    pub fn filter_constraint(&self, req: &str, constraint: Box<dyn ConstraintInterface>, allow_upper_bound_override: bool) -> anyhow::Result<Box<dyn ConstraintInterface>> {
        if !PlatformRepository::is_platform_package(req) {
            return Ok(constraint);
        }

        if !allow_upper_bound_override || !Preg::is_match(&self.ignore_upper_bound_regex, req)? {
            return Ok(constraint);
        }

        if Preg::is_match(&self.ignore_regex, req)? {
            return Ok(Box::new(MatchAllConstraint::new()));
        }

        let intervals = Intervals::get(&*constraint)?;
        let last = intervals.numeric.last();
        if let Some(last) = last {
            if last.get_end().to_string() != Interval::until_positive_infinity().to_string() {
                let constraint = Box::new(MultiConstraint::new(
                    vec![constraint, Box::new(Constraint::new(">=", last.get_end().get_version()))],
                    false,
                ));
                return Ok(constraint);
            }
        }

        Ok(constraint)
    }
}

impl PlatformRequirementFilterInterface for IgnoreListPlatformRequirementFilter {
    fn is_ignored(&self, req: &str) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        Preg::is_match(&self.ignore_regex, req).unwrap_or(false)
    }

    fn is_upper_bound_ignored(&self, req: &str) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        self.is_ignored(req) || Preg::is_match(&self.ignore_upper_bound_regex, req).unwrap_or(false)
    }
}
