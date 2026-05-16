#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLevel;

impl LogLevel {
    pub const EMERGENCY: &'static str = "emergency";
    pub const ALERT: &'static str = "alert";
    pub const CRITICAL: &'static str = "critical";
    pub const ERROR: &'static str = "error";
    pub const WARNING: &'static str = "warning";
    pub const NOTICE: &'static str = "notice";
    pub const INFO: &'static str = "info";
    pub const DEBUG: &'static str = "debug";
}
