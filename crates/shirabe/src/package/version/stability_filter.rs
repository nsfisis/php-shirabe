//! ref: composer/src/Composer/Package/Version/StabilityFilter.php

use crate::package::stability_priority;
use bstr::{BStr, BString};
use indexmap::IndexMap;

pub struct StabilityFilter;

impl StabilityFilter {
    pub fn is_package_acceptable(
        acceptable_stabilities: &IndexMap<BString, i64>,
        stability_flags: &IndexMap<BString, i64>,
        names: &[BString],
        stability: &BStr,
    ) -> bool {
        for name in names {
            // allow if package matches the package-specific stability flag
            if let Some(&flag) = stability_flags.get(name) {
                if let Some(stability_value) = stability_priority(stability)
                    && stability_value <= flag
                {
                    return true;
                }
            } else if acceptable_stabilities.contains_key(stability) {
                // allow if package matches the global stability requirement and has no exception
                return true;
            }
        }

        false
    }
}
