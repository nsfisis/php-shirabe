//! ref: composer/src/Composer/Util/PackageInfo.php

use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::package_interface::PackageInterface;

pub struct PackageInfo;

impl PackageInfo {
    pub fn get_view_source_url(package: &dyn PackageInterface) -> Option<String> {
        if let Some(complete) = package.as_complete_package_interface() {
            let support = complete.get_support();
            if let Some(source) = support.get("source") {
                if source != "" {
                    return Some(source.clone());
                }
            }
        }

        package.get_source_url()
    }

    pub fn get_view_source_or_homepage_url(package: &dyn PackageInterface) -> Option<String> {
        let url = Self::get_view_source_url(package).or_else(|| {
            package.as_complete_package_interface().and_then(|complete| complete.get_homepage())
        });

        if url.as_deref() == Some("") {
            return None;
        }

        url
    }
}
