//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreListPlatformRequirementFilter.php

use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::package::base_package::{self};
use crate::repository::PlatformRepository;
use bstr::{BStr, BString};
use shirabe_php_shim::preg_is_match;
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MultiConstraint, SimpleConstraint,
};
use shirabe_semver::{Interval, Intervals};

#[derive(Debug)]
pub struct IgnoreListPlatformRequirementFilter {
    ignore_regex: String,
    ignore_upper_bound_regex: String,
}

impl IgnoreListPlatformRequirementFilter {
    pub fn new(req_list: Vec<BString>) -> anyhow::Result<Self> {
        let mut ignore_all: Vec<BString> = Vec::new();
        let mut ignore_upper_bound: Vec<BString> = Vec::new();
        for req in req_list {
            if req.ends_with(b"+") {
                ignore_upper_bound.push(req[..req.len() - 1].into());
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
        req: &BStr,
        constraint: AnyConstraint,
        allow_upper_bound_override: bool,
    ) -> anyhow::Result<AnyConstraint> {
        if !PlatformRepository::is_platform_package(req) {
            return Ok(constraint);
        }

        if !allow_upper_bound_override || !preg_is_match(&self.ignore_upper_bound_regex, req) {
            return Ok(constraint);
        }

        if preg_is_match(&self.ignore_regex, req) {
            return Ok(MatchAllConstraint::new(None).into());
        }

        let intervals = Intervals::get(&constraint)?;
        let last = intervals.numeric.last();
        if let Some(last) = last
            && last.get_end().to_php_string() != Interval::until_positive_infinity().to_php_string()
        {
            let constraint = MultiConstraint::new(
                vec![
                    constraint,
                    AnyConstraint::Simple(SimpleConstraint::new(
                        ">=",
                        last.get_end().get_version(),
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
    fn is_ignored(&self, req: &BStr) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        preg_is_match(&self.ignore_regex, req)
    }

    fn is_upper_bound_ignored(&self, req: &BStr) -> bool {
        if !PlatformRepository::is_platform_package(req) {
            return false;
        }
        self.is_ignored(req) || preg_is_match(&self.ignore_upper_bound_regex, req)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
