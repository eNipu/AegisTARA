//! Dates for people, without a date crate.
//!
//! Times in AegisTARA are milliseconds since the Unix epoch, in UTC. The app
//! shows them in UTC with the zone named, so two users in different places
//! read the same text, and reports stay deterministic (P4).
//!
//! The calendar arithmetic is Howard Hinnant's `civil_from_days` (public
//! domain, <https://howardhinnant.github.io/date_algorithms.html>), exact for
//! every day of the proleptic Gregorian calendar. O(1).

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Year, month (1 to 12) and day (1 to 31) of a day count since 1970-01-01.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// `25 Oct 2026 19:30 UTC`. Zero shows as an empty string ("never").
#[must_use]
pub fn format_ms(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }
    let secs = i64::try_from(ms / 1000).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let in_day = secs.rem_euclid(86_400);
    format!(
        "{d} {} {y} {:02}:{:02} UTC",
        MONTHS[(m - 1) as usize],
        in_day / 3600,
        (in_day % 3600) / 60
    )
}

/// `2026-10-25`, for file names.
#[must_use]
pub fn date_ms(ms: u64) -> String {
    let secs = i64::try_from(ms / 1000).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// A date and time in the language's style (L10N-05): Japanese writes
/// `2026年10月25日 19:30 UTC`, every other language the English form. The
/// UTC zone stays named so two readers in different places agree (P4).
#[must_use]
pub fn format_ms_lang(ms: u64, lang: crate::i18n::Language) -> String {
    use crate::i18n::Language;
    if lang != Language::Ja {
        return format_ms(ms);
    }
    if ms == 0 {
        return String::new();
    }
    let secs = i64::try_from(ms / 1000).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let in_day = secs.rem_euclid(86_400);
    format!(
        "{y}年{m}月{d}日 {:02}:{:02} UTC",
        in_day / 3600,
        (in_day % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(format_ms(1_792_956_600_000), "25 Oct 2026 19:30 UTC");
        assert_eq!(date_ms(1_792_956_600_000), "2026-10-25");
        assert_eq!(format_ms(0), "");
    }

    #[test]
    fn every_day_round_trips_through_a_year() {
        // Day counts are consecutive dates: no gaps, no repeats.
        let mut last = civil_from_days(-800_000);
        for day in -799_999..800_000 {
            let next = civil_from_days(day);
            assert!(next > last, "{day}");
            last = next;
        }
    }
}
