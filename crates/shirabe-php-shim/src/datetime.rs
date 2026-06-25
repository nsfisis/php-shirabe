static DEFAULT_TIMEZONE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn date_create<Tz: chrono::TimeZone>(s: &str) -> chrono::ParseResult<chrono::DateTime<Tz>> {
    // TODO(phase-d): PHP `date_create` accepts the full strtotime() grammar (RFC2822, ISO8601,
    // VCS-specific and relative formats), which requires a dedicated date parser not available
    // here. The generic `Tz` also has no constructor from a parsed `FixedOffset`/`Utc` value.
    let _ = s;
    todo!()
}

/// PHP: \DATE_RFC3339 ("Y-m-d\TH:i:sP").
pub const DATE_RFC3339: &str = "%Y-%m-%dT%H:%M:%S%:z";

/// PHP: \DATE_ATOM (equivalent to \DATE_RFC3339).
pub const DATE_ATOM: &str = DATE_RFC3339;

/// Convert PHP-compatible date time format to strftime-compatible format.
/// Only the patterns Composer actually passes are supported; anything else panics.
pub fn date_format_to_strftime(format: &str) -> &'static str {
    match format {
        "Y-m-d H:i:s" => "%Y-%m-%d %H:%M:%S",
        "Y-m-d Hi" => "%Y-%m-%d %H%M",
        "Y-m-d" => "%Y-%m-%d",
        "Ymd" => "%Y%m%d",
        other => panic!("Unsupported PHP date format: {other:?}"),
    }
}

pub fn strtotime(_time: &str) -> Option<i64> {
    // TODO(phase-d): requires the full strtotime() grammar (absolute, relative and compound
    // expressions); a partial parser would silently mis-handle unsupported inputs.
    todo!()
}

pub fn time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn microtime() -> f64 {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    duration.as_secs_f64()
}

// PHP defaults to "UTC" when no default timezone has been configured.
pub fn date_default_timezone_get() -> String {
    DEFAULT_TIMEZONE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "UTC".to_string())
}

pub fn date_default_timezone_set(tz: &str) -> bool {
    *DEFAULT_TIMEZONE.lock().unwrap() = Some(tz.to_string());
    true
}

pub fn date(format: &str, timestamp: Option<i64>) -> String {
    let timestamp = timestamp.unwrap_or_else(time);
    // PHP `date()` renders in the default timezone. Without a timezone database only "UTC" can be
    // resolved; any named zone is rejected loudly rather than silently rendered in the wrong zone.
    let tz = date_default_timezone_get();
    if tz != "UTC" {
        panic!(
            "date() with non-UTC default timezone {tz:?} is not supported (no timezone database)"
        );
    }
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .expect("date() timestamp out of range");
    dt.format(date_format_to_strftime(format)).to_string()
}
