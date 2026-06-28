//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreListPlatformRequirementFilter.php

use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::package::base_package::{self};
use crate::repository::PlatformRepository;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_semver::Interval;
use shirabe_semver::Intervals;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MatchAllConstraint;
use shirabe_semver::constraint::MultiConstraint;
use shirabe_semver::constraint::SimpleConstraint;

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
        let ignore_regex = base_package::package_names_to_regexp(&ignore_all, "{^(?:%s)$}iD");
        let ignore_upper_bound_regex =
            base_package::package_names_to_regexp(&ignore_upper_bound, "{^(?:%s)$}iD");
        Ok(Self {
            ignore_regex,
            ignore_upper_bound_regex,
        })
    }

    pub fn filter_constraint(
        &self,
        req: &str,
        constraint: AnyConstraint,
        allow_upper_bound_override: bool,
    ) -> anyhow::Result<AnyConstraint> {
        if !PlatformRepository::is_platform_package(req) {
            return Ok(constraint);
        }

        if !allow_upper_bound_override || !Preg::is_match(&self.ignore_upper_bound_regex, req) {
            return Ok(constraint);
        }

        if Preg::is_match(&self.ignore_regex, req) {
            return Ok(MatchAllConstraint::new(None).into());
        }

        let intervals = Intervals::get(&constraint)?;
        let last = intervals.numeric.last();
        if let Some(last) = last
            && last.get_end().to_string() != Interval::until_positive_infinity().to_string()
        {
            let constraint = MultiConstraint::new(
                vec![
                    constraint,
                    AnyConstraint::Simple(SimpleConstraint::new(
                        ">=".to_string(),
                        last.get_end().get_version().to_string(),
                        None,
                    )),
                ],
                false,
                None,
            )
            .into();
            return Ok(constraint);
        }

        Ok(constraint)
    }
}

impl PlatformRequirementFilterInterface for IgnoreListPlatformRequirementFilter {
    fn is_ignored(&self, req: &str) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        Preg::is_match(&self.ignore_regex, req)
    }

    fn is_upper_bound_ignored(&self, req: &str) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        self.is_ignored(req) || Preg::is_match(&self.ignore_upper_bound_regex, req)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
