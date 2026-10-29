//! ref: composer/src/Composer/Downloader/ChangeReportInterface.php

use crate::package::PackageInterfaceHandle;

#[async_trait::async_trait(?Send)]
pub trait ChangeReportInterface {
    async fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>>;
}
