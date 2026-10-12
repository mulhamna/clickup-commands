//! Human date input → unix ms, durations, and display helpers.
//! `tz` is an IANA name (e.g. `Asia/Jakarta`); `None` or an unknown name
//! falls back to the system timezone.

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
    Weekday,
};
use chrono_tz::Tz;

use crate::error::{ClickUpError, Result};

enum Zone {
    Named(Tz),
    System,
}

fn zone(tz: Option<&str>) -> Zone {
    tz.and_then(|s| s.parse::<Tz>().ok())
        .map(Zone::Named)
        .unwrap_or(Zone::System)
}

impl Zone {
    fn local(&self, utc: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Named(tz) => utc.with_timezone(tz).naive_local(),
            Zone::System => utc.with_timezone(&Local).naive_local(),
        }
    }

    fn to_ms(&self, naive: NaiveDateTime) -> i64 {
        match self {
            Zone::Named(tz) => tz
                .from_local_datetime(&naive)
                .earliest()
                .map(|d| d.timestamp_millis())
                .unwrap_or_else(|| tz.from_utc_datetime(&naive).timestamp_millis()),
            Zone::System => Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|d| d.timestamp_millis())
                .unwrap_or_else(|| Local.from_utc_datetime(&naive).timestamp_millis()),
        }
    }

    fn rfc3339(&self, utc: DateTime<Utc>) -> String {
        match self {
            Zone::Named(tz) => utc.with_timezone(tz).to_rfc3339(),
            Zone::System => utc.with_timezone(&Local).to_rfc3339(),
        }
    }
}

/// True for IANA names like `Asia/Jakarta` or `UTC`.
pub fn is_valid_tz(name: &str) -> bool {
    name.parse::<Tz>().is_ok()
}

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Parse `2026-10-20`, `2026-10-20 14:00`, RFC 3339, `now`, `today`,
/// `tomorrow`, `yesterday`, `mon`..`sun` (next occurrence), `+3d`, `-2w`,
/// `+4h`, `+30m`, `eod`, `eow`, or a raw unix-ms number.
pub fn parse_when(input: &str, tz: Option<&str>) -> Result<i64> {
    parse_when_at(input, tz, Utc::now())
}

pub fn parse_when_at(input: &str, tz: Option<&str>, now: DateTime<Utc>) -> Result<i64> {
    let s = input.trim().to_ascii_lowercase();
    let z = zone(tz);
    let local_now = z.local(now);
    let today = local_now.date();
    let midnight = |d: NaiveDate| z.to_ms(d.and_time(NaiveTime::MIN));
    let end_of = |d: NaiveDate| {
        z.to_ms(
            d.and_hms_milli_opt(23, 59, 59, 999)
                .expect("valid end of day"),
        )
    };
    let invalid = || {
        ClickUpError::Invalid(format!(
            "cannot parse date '{input}' (try 2026-10-20, '2026-10-20 14:00', today, tomorrow, fri, +3d, eow)"
        ))
    };

    match s.as_str() {
        "" => return Err(invalid()),
        "now" => return Ok(now.timestamp_millis()),
        "today" => return Ok(midnight(today)),
        "tomorrow" => return Ok(midnight(today + Duration::days(1))),
        "yesterday" => return Ok(midnight(today - Duration::days(1))),
        "eod" => return Ok(end_of(today)),
        "eow" => {
            let to_sunday = 6 - i64::from(today.weekday().num_days_from_monday());
            return Ok(end_of(today + Duration::days(to_sunday)));
        }
        _ => {}
    }

    if let Some(wd) = weekday(&s) {
        let diff = (7 + i64::from(wd.num_days_from_monday())
            - i64::from(today.weekday().num_days_from_monday()))
            % 7;
        let diff = if diff == 0 { 7 } else { diff };
        return Ok(midnight(today + Duration::days(diff)));
    }

    if let Some(sign) = s.chars().next().filter(|c| *c == '+' || *c == '-') {
        // `sign` is ASCII, so slicing after it is safe; the unit may be any
        // char, so strip it by its UTF-8 length rather than by one byte.
        let body = &s[1..];
        let unit = body.chars().last().ok_or_else(invalid)?;
        let n: i64 = body[..body.len() - unit.len_utf8()]
            .parse()
            .map_err(|_| invalid())?;
        let n = if sign == '-' { -n } else { n };
        let delta = match unit {
            'm' => Duration::try_minutes(n),
            'h' => Duration::try_hours(n),
            'd' => Duration::try_days(n),
            'w' => Duration::try_weeks(n),
            _ => return Err(invalid()),
        }
        .ok_or_else(invalid)?;
        return now
            .checked_add_signed(delta)
            .map(|t| t.timestamp_millis())
            .ok_or_else(invalid);
    }

    if s.len() >= 12 && s.chars().all(|c| c.is_ascii_digit()) {
        return s.parse().map_err(|_| invalid());
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(input.trim()) {
        return Ok(dt.timestamp_millis());
    }
    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(midnight(d));
    }
    for fmt in [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dt%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dt%H:%M:%S",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s, fmt) {
            return Ok(z.to_ms(dt));
        }
    }
    Err(invalid())
}

fn weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "wednesday" => Weekday::Wed,
        "thu" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    })
}

/// `1h30m`, `1h 30m`, `45m`, `2h`, `1d`, `90s` → milliseconds.
pub fn parse_duration(input: &str) -> Result<i64> {
    let invalid = || {
        ClickUpError::Invalid(format!(
            "cannot parse duration '{input}' (try 1h30m, 45m, 2h)"
        ))
    };
    let s: String = input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if s.is_empty() {
        return Err(invalid());
    }
    let mut total = 0i64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: i64 = num.parse().map_err(|_| invalid())?;
        num.clear();
        let unit_ms: i64 = match c {
            'd' => 86_400_000,
            'h' => 3_600_000,
            'm' => 60_000,
            's' => 1_000,
            _ => return Err(invalid()),
        };
        total = n
            .checked_mul(unit_ms)
            .and_then(|ms| total.checked_add(ms))
            .ok_or_else(invalid)?;
    }
    if !num.is_empty() || total <= 0 {
        return Err(invalid());
    }
    Ok(total)
}

/// `5400000` → `1h 30m`.
pub fn format_duration(ms: i64) -> String {
    let neg = ms < 0;
    let secs = ms.abs() / 1000;
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let body = match (h, m) {
        (0, 0) if s > 0 => format!("{s}s"),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// `2026-10-20 14:00`, or just `2026-10-20` at local midnight.
pub fn format_ms(ms: i64, tz: Option<&str>) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp_millis(ms) else {
        return ms.to_string();
    };
    let local = zone(tz).local(utc);
    if local.time() == NaiveTime::MIN {
        local.format("%Y-%m-%d").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M").to_string()
    }
}

/// RFC 3339 with the configured offset (for machine-readable output).
pub fn to_iso(ms: i64, tz: Option<&str>) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|utc| zone(tz).rfc3339(utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TZ: Option<&str> = Some("Asia/Jakarta"); // UTC+7, no DST

    fn now() -> DateTime<Utc> {
        // Wednesday 2026-10-14 10:00 in Jakarta.
        Utc.with_ymd_and_hms(2026, 10, 14, 3, 0, 0).unwrap()
    }

    fn jkt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        let tz: Tz = "Asia/Jakarta".parse().unwrap();
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn absolute_dates() {
        assert_eq!(
            parse_when_at("2026-10-20", TZ, now()).unwrap(),
            jkt(2026, 10, 20, 0, 0)
        );
        assert_eq!(
            parse_when_at("2026-10-20 14:00", TZ, now()).unwrap(),
            jkt(2026, 10, 20, 14, 0)
        );
        assert_eq!(
            parse_when_at("2026-10-20T14:30", TZ, now()).unwrap(),
            jkt(2026, 10, 20, 14, 30)
        );
        assert_eq!(
            parse_when_at("1790000000000", TZ, now()).unwrap(),
            1_790_000_000_000
        );
    }

    #[test]
    fn relative_dates() {
        assert_eq!(
            parse_when_at("today", TZ, now()).unwrap(),
            jkt(2026, 10, 14, 0, 0)
        );
        assert_eq!(
            parse_when_at("tomorrow", TZ, now()).unwrap(),
            jkt(2026, 10, 15, 0, 0)
        );
        assert_eq!(
            parse_when_at("fri", TZ, now()).unwrap(),
            jkt(2026, 10, 16, 0, 0)
        );
        // Same weekday → next week.
        assert_eq!(
            parse_when_at("wed", TZ, now()).unwrap(),
            jkt(2026, 10, 21, 0, 0)
        );
        assert_eq!(
            parse_when_at("+3d", TZ, now()).unwrap(),
            jkt(2026, 10, 17, 10, 0)
        );
        assert_eq!(
            parse_when_at("-2w", TZ, now()).unwrap(),
            jkt(2026, 9, 30, 10, 0)
        );
        assert_eq!(
            parse_when_at("eow", TZ, now()).unwrap(),
            jkt(2026, 10, 18, 23, 59) + 59_999
        );
        assert!(parse_when_at("someday", TZ, now()).is_err());
        // Regressions: lone sign, and a multibyte char where the unit goes
        // (found by fuzzing: bytes 2d 00 f6 d9 df).
        for bad in ["+", "-", "+d", "-é", "+3é", "-\u{0}\u{fffd}\u{fffd}"] {
            assert!(parse_when_at(bad, TZ, now()).is_err(), "{bad:?}");
        }
        let fuzzed = String::from_utf8_lossy(&[0x2d, 0x00, 0xf6, 0xd9, 0xdf]).to_string();
        assert!(parse_when_at(&fuzzed, TZ, now()).is_err());
        // Overflow regressions (fuzz input "-6662020166202017h").
        for big in [
            "-6662020166202017h",
            "+9223372036854775807w",
            "-99999999999999d",
        ] {
            assert!(parse_when_at(big, TZ, now()).is_err(), "{big:?}");
        }
        assert!(parse_when_at("+3x", TZ, now()).is_err());
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("1h30m").unwrap(), 5_400_000);
        assert_eq!(parse_duration("1h 30m").unwrap(), 5_400_000);
        assert_eq!(parse_duration("45m").unwrap(), 2_700_000);
        assert_eq!(parse_duration("1d").unwrap(), 86_400_000);
        assert!(parse_duration("90").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("9223372036854775807d").is_err());
        assert!(parse_duration("3000000000000h3000000000000h").is_err());
        assert_eq!(format_duration(5_400_000), "1h 30m");
        assert_eq!(format_duration(7_200_000), "2h");
        assert_eq!(format_duration(30_000), "30s");
        assert_eq!(format_duration(0), "0m");
    }

    #[test]
    fn timezones() {
        assert!(is_valid_tz("Asia/Jakarta"));
        assert!(is_valid_tz("UTC"));
        assert!(!is_valid_tz("Mars/Olympus"));
    }

    #[test]
    fn formatting() {
        assert_eq!(format_ms(jkt(2026, 10, 20, 0, 0), TZ), "2026-10-20");
        assert_eq!(format_ms(jkt(2026, 10, 20, 14, 5), TZ), "2026-10-20 14:05");
        assert_eq!(
            to_iso(jkt(2026, 10, 20, 14, 5), TZ).unwrap(),
            "2026-10-20T14:05:00+07:00"
        );
    }
}
