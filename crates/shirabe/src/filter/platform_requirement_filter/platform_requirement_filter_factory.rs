//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterFactory.php

use anyhow::Result;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};
use crate::filter::platform_requirement_filter::{
    ignore_all_platform_requirement_filter::IgnoreAllPlatformRequirementFilter,
    ignore_list_platform_requirement_filter::IgnoreListPlatformRequirementFilter,
    ignore_nothing_platform_requirement_filter::IgnoreNothingPlatformRequirementFilter,
    platform_requirement_filter_interface::PlatformRequirementFilterInterface,
};

pub struct PlatformRequirementFilterFactory;

impl PlatformRequirementFilterFactory {
    pub fn from_bool_or_list(bool_or_list: PhpMixed) -> Result<Box<dyn PlatformRequirementFilterInterface>> {
        match bool_or_list {
            PhpMixed::Bool(b) => {
                if b {
                    Ok(Self::ignore_all())
                } else {
                    Ok(Self::ignore_nothing())
                }
            }
            list_or_array @ (PhpMixed::List(_) | PhpMixed::Array(_)) => {
                Ok(Box::new(IgnoreListPlatformRequirementFilter::new(list_or_array)))
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

    pub fn ignore_all() -> Box<dyn PlatformRequirementFilterInterface> {
        Box::new(IgnoreAllPlatformRequirementFilter)
    }

    pub fn ignore_nothing() -> Box<dyn PlatformRequirementFilterInterface> {
        Box::new(IgnoreNothingPlatformRequirementFilter)
    }
}
