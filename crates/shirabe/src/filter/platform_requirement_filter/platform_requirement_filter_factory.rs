//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterFactory.php

use crate::filter::platform_requirement_filter::{
    ignore_all_platform_requirement_filter::IgnoreAllPlatformRequirementFilter,
    ignore_list_platform_requirement_filter::IgnoreListPlatformRequirementFilter,
    ignore_nothing_platform_requirement_filter::IgnoreNothingPlatformRequirementFilter,
    platform_requirement_filter_interface::PlatformRequirementFilterInterface,
};
use anyhow::Result;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};
use std::rc::Rc;

pub struct PlatformRequirementFilterFactory;

impl PlatformRequirementFilterFactory {
    pub fn from_bool_or_list(
        bool_or_list: PhpMixed,
    ) -> Result<Rc<dyn PlatformRequirementFilterInterface>> {
        match bool_or_list {
            PhpMixed::Bool(b) => {
                if b {
                    Ok(Self::ignore_all())
                } else {
                    Ok(Self::ignore_nothing())
                }
            }
            list_or_array @ (PhpMixed::List(_) | PhpMixed::Array(_)) => {
                let list: Vec<String> = match list_or_array {
                    PhpMixed::List(items) => items
                        .into_iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect(),
                    PhpMixed::Array(map) => map
                        .into_iter()
                        .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                        .collect(),
                    _ => unreachable!(),
                };
                Ok(Rc::new(IgnoreListPlatformRequirementFilter::new(list)?))
            }
            other => Err(anyhow::anyhow!(InvalidArgumentException {
                message: format!(
                    "PlatformRequirementFilter: Unknown $boolOrList parameter {}. Please report at https://github.com/composer/composer/issues/new.",
                    shirabe_php_shim::get_debug_type(&other)
                ),
                code: 0,
            })),
        }
    }

    pub fn ignore_all() -> Rc<dyn PlatformRequirementFilterInterface> {
        Rc::new(IgnoreAllPlatformRequirementFilter)
    }

    pub fn ignore_nothing() -> Rc<dyn PlatformRequirementFilterInterface> {
        Rc::new(IgnoreNothingPlatformRequirementFilter)
    }
}
