//! ref: composer/src/Composer/Plugin/PluginEvents.php

pub struct PluginEvents;

impl PluginEvents {
    pub const INIT: &'static str = "init";
    pub const COMMAND: &'static str = "command";
    pub const PRE_FILE_DOWNLOAD: &'static str = "pre-file-download";
    pub const POST_FILE_DOWNLOAD: &'static str = "post-file-download";
    pub const PRE_COMMAND_RUN: &'static str = "pre-command-run";
    pub const PRE_POOL_CREATE: &'static str = "pre-pool-create";
}
