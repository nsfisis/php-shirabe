//! ref: composer/src/Composer/Util/PackageInfo.php sha1:04c300c3bf0fc2af9af2f768802c05e0a257d660

use crate::package::PackageInterfaceHandle;
use bstr::BString;

pub struct PackageInfo;

impl PackageInfo {
    pub fn get_view_source_url(package: PackageInterfaceHandle) -> Option<BString> {
        if let Some(complete) = package.as_complete() {
            let support = complete.get_support();
            if let Some(source) = support.get(bstr::B("source"))
                && !source.is_empty()
            {
                return Some(source.clone());
            }
        }

        package.get_source_url()
    }

    pub fn get_view_source_or_homepage_url(package: PackageInterfaceHandle) -> Option<BString> {
        let url = Self::get_view_source_url(package.clone()).or_else(|| {
            package
                .as_complete()
                .and_then(|complete| complete.get_homepage())
        });

        if url.as_ref().is_some_and(|url| url.is_empty()) {
            return None;
        }

        url
    }
}
