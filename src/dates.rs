//! Calendar dates and times without a dependency: the clock, RFC 3339 and
//! ISO dates, and the short forms the Topics window shows.

/// Day names from 1970-01-01, a Thursday.
const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// The weekday of a day number (days since 1970-01-01).
pub fn weekday(days: i64) -> &'static str {
    WEEKDAYS[days.rem_euclid(7) as usize]
}

/// The month of a 1-based month number.
pub fn month(m: i64) -> &'static str {
    MONTHS[(m.clamp(1, 12) - 1) as usize]
}

/// Unix seconds.
pub fn now() -> i64 {
    (now_ms() / 1000) as i64
}

/// Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// (year, month, day) of a day number.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `2026-11-24` (or `2026-11`) → day number.
pub fn parse_date(s: &str) -> Option<i64> {
    let y = s.get(0..4)?.parse().ok()?;
    let m = s.get(5..7)?.parse().ok()?;
    let d = s.get(8..10).and_then(|d| d.parse().ok()).unwrap_or(1);
    Some(days_from_civil(y, m, d))
}

/// RFC 3339 → Unix seconds.
pub fn parse_time(s: &str) -> Option<i64> {
    let days = parse_date(s)?;
    let hms = |a: usize, b: usize| s.get(a..b).and_then(|v| v.parse::<i64>().ok());
    let secs = hms(11, 13).unwrap_or(0) * 3600 + hms(14, 16).unwrap_or(0) * 60 + hms(17, 19).unwrap_or(0);
    let offset = s
        .rfind(['+', '-'])
        .filter(|&i| i > 18)
        .and_then(|i| {
            let sign = if &s[i..=i] == "-" { -1 } else { 1 };
            let h: i64 = s.get(i + 1..i + 3)?.parse().ok()?;
            let m: i64 = s.get(i + 4..i + 6)?.parse().ok()?;
            Some(sign * (h * 3600 + m * 60))
        })
        .unwrap_or(0);
    Some(days * 86400 + secs - offset)
}

/// `2026-11-25T00:12:03Z`.
pub fn rfc3339(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86400));
    let s = unix.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

/// The time now as `rfc3339`.
pub fn stamp() -> String {
    rfc3339(now())
}

/// `2026-11-24`.
pub fn iso_date(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `Tuesday 24 November`.
pub fn long_date(iso: &str) -> String {
    let Some(days) = parse_date(iso) else {
        return iso.into();
    };
    let (_, m, d) = civil_from_days(days);
    format!("{} {d} {}", weekday(days), month(m))
}

/// `24 Nov`, or `Apr 2027` for a month.
pub fn day_month(iso: &str) -> String {
    match parse_date(iso) {
        Some(days) if iso.len() >= 10 => {
            let (_, m, d) = civil_from_days(days);
            format!("{d} {}", &month(m)[..3])
        }
        Some(days) => {
            let (y, m, _) = civil_from_days(days);
            format!("{} {y}", &month(m)[..3])
        }
        None => iso.into(),
    }
}

/// The two lines of a calendar chip: weekday within four weeks of `today`,
/// month after that (`THU` / `26`, `MAR` / `25`).
pub fn chip(iso: &str, today: &str) -> Option<(String, String)> {
    let (day, t) = (parse_date(iso)?, parse_date(today)?);
    let (_, m, d) = civil_from_days(day);
    let top = if (0..28).contains(&(day - t)) { weekday(day) } else { month(m) };
    Some((top[..3].to_uppercase(), d.to_string()))
}

/// How long ago, briefly: `now`, `12 min`, `3 h`, `Yesterday`, `24 Nov`.
pub fn ago(now: i64, t: i64) -> String {
    let s = now - t;
    match s {
        i64::MIN..60 => "now".into(),
        60..3600 => format!("{} min", s / 60),
        3600..86400 => format!("{} h", s / 3600),
        86400..172800 => "Yesterday".into(),
        _ => day_month(&iso_date(t)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip_and_name_weekdays() {
        let d = parse_date("2026-11-24").unwrap();
        assert_eq!(iso_date(d * 86400), "2026-11-24");
        assert_eq!(long_date("2026-11-24"), "Tuesday 24 November");
        assert_eq!(chip("2026-11-26", "2026-11-24"), Some(("THU".into(), "26".into())));
        assert_eq!(chip("2027-03-25", "2026-11-24"), Some(("MAR".into(), "25".into())));
        let t = parse_time("2026-11-24T21:31:28.630289629+00:00").unwrap();
        assert_eq!(parse_time("2026-11-24T22:31:28+01:00"), Some(t));
        assert_eq!(rfc3339(t), "2026-11-24T21:31:28Z");
        assert_eq!(ago(t + 3 * 3600, t), "3 h");
        assert_eq!(ago(t + 3 * 86400, t), "24 Nov");
        assert_eq!(day_month("2027-04"), "Apr 2027");
    }
}
