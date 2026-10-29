//! ref: composer/src/Composer/Util/PackageInfo.php

use crate::package::PackageInterfaceHandle;
use shirabe_php_shim::legacy_lossy;

pub struct PackageInfo;

impl PackageInfo {
    pub fn get_view_source_url(package: PackageInterfaceHandle) -> Option<String> {
        if let Some(complete) = package.as_complete() {
            let support = complete.get_support();
            if let Some(source) = support.get(bstr::B("source"))
                && !source.is_empty()
            {
                return Some(legacy_lossy(&source.clone()).into_owned());
            }
        }

        package
            .get_source_url()
            .map(|s| legacy_lossy(&s).into_owned())
    }

    pub fn get_view_source_or_homepage_url(package: PackageInterfaceHandle) -> Option<String> {
        let url = Self::get_view_source_url(package.clone()).or_else(|| {
            package
                .as_complete()
                .and_then(|complete| complete.get_homepage())
                .map(|s| legacy_lossy(&s).into_owned())
        });

        if url.as_deref() == Some("") {
            return None;
        }

        url
    }
}
