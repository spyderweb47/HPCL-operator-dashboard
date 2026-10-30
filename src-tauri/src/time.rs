//! Business-day and timestamp helpers.

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, TimeDelta, Utc};

/// India Standard Time (UTC+05:30, no daylight saving). The business day an entry belongs to is
/// decided here, not by the Windows clock's timezone. Must match APP_TIMEZONE in src/lib/date.ts.
pub const BUSINESS_UTC_OFFSET_SECS: i32 = 5 * 3600 + 30 * 60;

/// How many days of history stay on this PC (older, already-synced days are pruned).
pub const RETENTION_DAYS: i64 = 180;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Today's business date as YYYY-MM-DD.
pub fn today() -> String {
    let tz = FixedOffset::east_opt(BUSINESS_UTC_OFFSET_SECS).expect("valid offset");
    Utc::now().with_timezone(&tz).format("%Y-%m-%d").to_string()
}

pub fn is_valid_date(d: &str) -> bool {
    d.len() == 10 && NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
}

pub fn shift_date(d: &str, days: i64) -> String {
    NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .map(|n| (n + TimeDelta::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| d.to_string())
}

/// First date still kept locally (inclusive).
pub fn retention_cutoff(today: &str) -> String {
    shift_date(today, -(RETENTION_DAYS - 1))
}

pub fn iso_to_millis(s: &str) -> i64 {
    DateTime::parse_from_rfc3339(s).map(|d| d.timestamp_millis()).unwrap_or(0)
}

pub fn millis_to_iso(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}
