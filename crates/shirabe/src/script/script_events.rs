//! ref: composer/src/Composer/Script/ScriptEvents.php

pub struct ScriptEvents;

impl ScriptEvents {
    pub const PRE_INSTALL_CMD: &'static str = "pre-install-cmd";
    pub const POST_INSTALL_CMD: &'static str = "post-install-cmd";
    pub const PRE_UPDATE_CMD: &'static str = "pre-update-cmd";
    pub const POST_UPDATE_CMD: &'static str = "post-update-cmd";
    pub const PRE_STATUS_CMD: &'static str = "pre-status-cmd";
    pub const POST_STATUS_CMD: &'static str = "post-status-cmd";
    pub const PRE_AUTOLOAD_DUMP: &'static str = "pre-autoload-dump";
    pub const POST_AUTOLOAD_DUMP: &'static str = "post-autoload-dump";
    pub const POST_ROOT_PACKAGE_INSTALL: &'static str = "post-root-package-install";
    pub const POST_CREATE_PROJECT_CMD: &'static str = "post-create-project-cmd";
    pub const PRE_ARCHIVE_CMD: &'static str = "pre-archive-cmd";
    pub const POST_ARCHIVE_CMD: &'static str = "post-archive-cmd";

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
