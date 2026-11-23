//! RFC 3339 date-times (`2026-11-24T09:30:05Z`) without a date crate.

/// The current time in Unix seconds.
pub fn now() -> i64 {
    let since_1970 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since_1970.map_or(0, |since| since.as_secs() as i64)
}

/// Unix seconds to a UTC date-time string.
pub fn format(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86_400));
    let secs = unix.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// A date-time string to Unix seconds (a fraction of a second is dropped).
/// `None` if the string is not an RFC 3339 date-time.
pub fn parse(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = b.get(from..to)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };

    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let separators_ok = b[4] == b'-' && b[7] == b'-' && matches!(b[10], b'T' | b't') && b[13] == b':' && b[16] == b':';
    let ranges_ok = (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 60; // 60 is a leap second
    if !separators_ok || !ranges_ok {
        return None;
    }

    // Optional fraction of a second.
    let mut i = 19;
    if b[i] == b'.' {
        let digits = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        i += 1 + digits;
    }

    // Offset from UTC: `Z` or `+HH:MM` / `-HH:MM`.
    let offset = match *b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let (h, m) = (num(i + 1, i + 3)?, num(i + 4, i + 6)?);
            if h > 23 || m > 59 {
                return None;
            }
            let secs = h * 3600 + m * 60;
            if sign == b'+' { secs } else { -secs }
        }
        _ => return None,
    };

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// The two functions below are Howard Hinnant's public-domain calendar algorithms.

/// Days since 1970-01-01 to (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// (year, month, day) to days since 1970-01-01.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_moments() {
        assert_eq!(format(0), "1970-01-01T00:00:00Z");
        assert_eq!(format(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(format(1_795_512_605), "2026-11-24T09:30:05Z");
        assert_eq!(format(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn parse_is_the_reverse_of_format() {
        for unix in (-100_000_000..2_000_000_000).step_by(9_876_543) {
            assert_eq!(parse(&format(unix)), Some(unix), "{}", format(unix));
        }
    }

    #[test]
    fn parses_offsets_and_fractions() {
        assert_eq!(parse("2026-11-24T09:30:05Z"), Some(1_795_512_605));
        assert_eq!(parse("2026-11-24t09:30:05z"), Some(1_795_512_605));
        assert_eq!(parse("2026-11-24T09:30:05.250Z"), Some(1_795_512_605));
        assert_eq!(parse("2026-11-24T22:30:05+13:00"), Some(1_795_512_605));
        assert_eq!(parse("2026-11-24T04:00:05.5-05:30"), Some(1_795_512_605));
    }

    #[test]
    fn refuses_what_is_not_a_date_time() {
        for bad in [
            "",
            "2026-11-24",
            "2026-11-24T09:30:05",
            "2026-11-24 09:30:05Z",
            "2026-13-24T09:30:05Z",
            "2026-02-30T09:30:05Z",
            "2025-02-29T09:30:05Z",
            "2026-11-24T24:00:00Z",
            "2026-11-24T09:60:05Z",
            "2026-11-24T09:30:05.Z",
            "2026-11-24T09:30:05+1300",
            "2026-11-24T09:30:05+24:00",
            "2026-11-24T09:30:05Zjunk",
            "2026/11/24T09:30:05Z",
            "yesterday at about nine",
            "２０２６-11-24T09:30:05Z",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }
}
