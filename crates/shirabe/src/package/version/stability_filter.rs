//! ref: composer/src/Composer/Package/Version/StabilityFilter.php

use crate::package::base_package::BasePackage;
use indexmap::IndexMap;

pub struct StabilityFilter;

impl StabilityFilter {
    pub fn is_package_acceptable(
        acceptable_stabilities: &IndexMap<String, i64>,
        stability_flags: &IndexMap<String, i64>,
        names: &[String],
        stability: &str,
    ) -> bool {
        for name in names {
            // allow if package matches the package-specific stability flag
            if let Some(&flag) = stability_flags.get(name) {
                if let Some(&stability_value) = BasePackage::STABILITIES.get(stability) {
                    if stability_value <= flag {
                        return true;
                    }
                }
            } else if acceptable_stabilities.contains_key(stability) {
                // allow if package matches the global stability requirement and has no exception
                return true;
            }
        }

        false
    }
}
