static DEFAULT_TIMEZONE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Parse the subset of the strtotime()/date_create() grammar that Composer actually emits.
///
/// Supported: ISO8601/RFC3339 (`2023-01-15T12:34:56Z`, `...+00:00`), `Y-m-d H:i:s`, `Y-m-d`,
/// and `@<unixtime>`. Inputs without an explicit offset are interpreted as UTC, matching the
/// default timezone this shim assumes elsewhere. Anything else returns `None` rather than
/// guessing, mirroring PHP returning `false` on unrecognized input.
fn parse_to_fixed(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // `@<unixtime>` (optionally with a fractional part) denotes a Unix timestamp in UTC.
    if let Some(rest) = s.strip_prefix('@') {
        let secs = if let Some((whole, _frac)) = rest.split_once('.') {
            whole.parse::<i64>().ok()?
        } else {
            rest.parse::<i64>().ok()?
        };
        let utc = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)?;
        return Some(utc.fixed_offset());
    }

    // RFC3339 / ISO8601 with an explicit offset or trailing `Z`.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt);
    }

    // `Y-m-d H:i:s` and `Y-m-d`, both interpreted as UTC.
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })?;
    let utc = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
    Some(utc.fixed_offset())
}

pub fn date_create<Tz: chrono::TimeZone>(s: &str) -> chrono::ParseResult<chrono::DateTime<Tz>>
where
    chrono::DateTime<Tz>: From<chrono::DateTime<chrono::FixedOffset>>,
{
    match parse_to_fixed(s) {
        Some(dt) => Ok(dt.into()),
        // PHP `date_create` returns `false` here; the closest faithful signal under chrono's
        // `ParseResult` is a parse error, produced by replaying a deliberately invalid parse.
        None => Err(chrono::DateTime::parse_from_rfc3339("").unwrap_err()),
    }
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
        "Y/m/d H:i:s" => "%Y/%m/%d %H:%M:%S",
        "Y-m-d Hi" => "%Y-%m-%d %H%M",
        "Y-m-d" => "%Y-%m-%d",
        "Ymd" => "%Y%m%d",
        other => panic!("Unsupported PHP date format: {other:?}"),
    }
}

/// Subset of strtotime() covering what Composer passes: `now`, the absolute formats handled by
/// `parse_to_fixed`, and simple single-unit relative offsets such as `-8 days` / `+3 hours`.
///
/// Returns `None` for anything else, matching PHP `strtotime()` returning `false`, rather than
/// silently mis-handling an unsupported expression.
pub fn strtotime(time: &str) -> Option<i64> {
    let trimmed = time.trim();

    if trimmed.eq_ignore_ascii_case("now") {
        return Some(self::time());
    }

    if let Some(dt) = parse_to_fixed(trimmed) {
        return Some(dt.timestamp());
    }

    parse_relative(trimmed).map(|delta| self::time() + delta)
}

/// Parse a single-unit relative offset like `-8 days`, `+3hours`, `1 week`, returning the signed
/// number of seconds. Whitespace between the count and unit is optional, as PHP accepts both.
fn parse_relative(s: &str) -> Option<i64> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic())
        .filter(|&i| i > 0)?;
    let (count_part, unit_part) = s.split_at(split);
    let count: i64 = count_part.trim().parse().ok()?;

    let unit = unit_part.trim().to_ascii_lowercase();
    let per_unit = match unit.as_str() {
        "sec" | "secs" | "second" | "seconds" => 1,
        "min" | "mins" | "minute" | "minutes" => 60,
        "hour" | "hours" => 60 * 60,
        "day" | "days" => 24 * 60 * 60,
        "week" | "weeks" => 7 * 24 * 60 * 60,
        _ => return None,
    };
    Some(count * per_unit)
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
