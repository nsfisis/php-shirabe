//! ref: composer/src/Composer/Script/ScriptEvents.php

use bstr::BStr;
use shirabe_php_shim::php_str;

pub struct ScriptEvents;

impl ScriptEvents {
    pub const PRE_INSTALL_CMD: &'static BStr = php_str(b"pre-install-cmd");
    pub const POST_INSTALL_CMD: &'static BStr = php_str(b"post-install-cmd");
    pub const PRE_UPDATE_CMD: &'static BStr = php_str(b"pre-update-cmd");
    pub const POST_UPDATE_CMD: &'static BStr = php_str(b"post-update-cmd");
    pub const PRE_STATUS_CMD: &'static BStr = php_str(b"pre-status-cmd");
    pub const POST_STATUS_CMD: &'static BStr = php_str(b"post-status-cmd");
    pub const PRE_AUTOLOAD_DUMP: &'static BStr = php_str(b"pre-autoload-dump");
    pub const POST_AUTOLOAD_DUMP: &'static BStr = php_str(b"post-autoload-dump");
    pub const POST_ROOT_PACKAGE_INSTALL: &'static BStr = php_str(b"post-root-package-install");
    pub const POST_CREATE_PROJECT_CMD: &'static BStr = php_str(b"post-create-project-cmd");
    pub const PRE_ARCHIVE_CMD: &'static BStr = php_str(b"pre-archive-cmd");
    pub const POST_ARCHIVE_CMD: &'static BStr = php_str(b"post-archive-cmd");

    pub fn is_defined(const_name: &[u8]) -> bool {
        matches!(
            const_name,
            b"PRE_INSTALL_CMD"
                | b"POST_INSTALL_CMD"
                | b"PRE_UPDATE_CMD"
                | b"POST_UPDATE_CMD"
                | b"PRE_STATUS_CMD"
                | b"POST_STATUS_CMD"
                | b"PRE_AUTOLOAD_DUMP"
                | b"POST_AUTOLOAD_DUMP"
                | b"POST_ROOT_PACKAGE_INSTALL"
                | b"POST_CREATE_PROJECT_CMD"
                | b"PRE_ARCHIVE_CMD"
                | b"POST_ARCHIVE_CMD"
        )
    }
}
