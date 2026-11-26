//! How the Topics window writes a date: the long form, the short forms, the
//! two lines of a calendar chip, and how long ago.
use margin_core::dates::{civil_from_days, iso_date, parse_date};

/// Day names from 1970-01-01, a Thursday.
const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// The weekday of a day number (days since 1970-01-01).
fn weekday(days: i64) -> &'static str {
    WEEKDAYS[days.rem_euclid(7) as usize]
}

/// The month of a 1-based month number.
fn month(m: i64) -> &'static str {
    MONTHS[(m.clamp(1, 12) - 1) as usize]
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
    use margin_core::dates::parse_time;

    #[test]
    fn dates_read_as_the_window_shows_them() {
        assert_eq!(long_date("2026-11-24"), "Tuesday 24 November");
        assert_eq!(chip("2026-11-26", "2026-11-24"), Some(("THU".into(), "26".into())));
        assert_eq!(chip("2027-03-25", "2026-11-24"), Some(("MAR".into(), "25".into())));
        let t = parse_time("2026-11-24T21:31:28+00:00").unwrap();
        assert_eq!(ago(t + 3 * 3600, t), "3 h");
        assert_eq!(ago(t + 3 * 86400, t), "24 Nov");
        assert_eq!(day_month("2027-04"), "Apr 2027");
    }
}
