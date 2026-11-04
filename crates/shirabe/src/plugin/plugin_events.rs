//! ref: composer/src/Composer/Plugin/PluginEvents.php sha1:e619b45d3b105a443ed909469cdd96cb9856d201

use bstr::BStr;
use shirabe_php_shim::php_str;

pub struct PluginEvents;

impl PluginEvents {
    pub const INIT: &'static BStr = php_str(b"init");
    pub const COMMAND: &'static BStr = php_str(b"command");
    pub const PRE_FILE_DOWNLOAD: &'static BStr = php_str(b"pre-file-download");
    pub const POST_FILE_DOWNLOAD: &'static BStr = php_str(b"post-file-download");
    pub const PRE_COMMAND_RUN: &'static BStr = php_str(b"pre-command-run");
    pub const PRE_POOL_CREATE: &'static BStr = php_str(b"pre-pool-create");
}
