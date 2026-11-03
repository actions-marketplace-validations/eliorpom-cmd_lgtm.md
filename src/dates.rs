//! Calendar dates as day numbers since 1970-01-01 (proleptic Gregorian), without a date crate.
//! All dates are UTC, like GitHub's contribution calendar.

/// Day number of a calendar date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Calendar date (year, month, day) of a day number.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// "2026-11-02" (or a longer timestamp starting with it) -> (2026, 11, 2).
pub fn parse_iso(iso: &str) -> (i64, i64, i64) {
    let part = |range: std::ops::Range<usize>| iso.get(range).and_then(|s| s.parse().ok()).unwrap_or(1);
    (part(0..4), part(5..7), part(8..10))
}

/// Day number of an ISO date.
pub fn day_number(iso: &str) -> i64 {
    let (y, m, d) = parse_iso(iso);
    days_from_civil(y, m, d)
}

/// "YYYY-MM-DD" of a day number.
pub fn iso(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Today's day number, in UTC.
pub fn today() -> i64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before 1970")
        .as_secs() as i64;
    seconds.div_euclid(86400)
}

/// "SUN" to "SAT" for an ISO date.
pub fn weekday_abbr(iso: &str) -> &'static str {
    // 1970-01-01 was a Thursday.
    ["THU", "FRI", "SAT", "SUN", "MON", "TUE", "WED"][day_number(iso).rem_euclid(7) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for day in [-1000, 0, 59, 60, 365, 10957, 20759, 20760, 30000] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        assert_eq!(iso(0), "1970-01-01");
        assert_eq!(iso(day_number("2026-11-03")), "2026-11-03");
        assert_eq!(iso(day_number("2024-02-29") + 1), "2024-03-01");
        assert_eq!(day_number("2026-11-02T19:49:02Z"), day_number("2026-11-02"));
        assert_eq!(weekday_abbr("2026-11-03"), "TUE");
        assert_eq!(weekday_abbr("2000-01-01"), "SAT");
    }
}
