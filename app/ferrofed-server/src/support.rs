// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The support period of the running release, compiled in by the release
//! build.
//!
//! Regulation (EU) 2024/2847 Art 13(8) gives each release a support period
//! of at least five years, and Art 13(19) has the manufacturer state its end
//! date and, where technically feasible, display a notification once the
//! release has reached it. The release lane sets `FERROFED_RELEASE_DATE` to
//! the date the release's changelog section carries, and the end date is
//! that date [`SUPPORT_YEARS`] years on, the date `SECURITY.md` lists for
//! the release. A development build and a pre-release are built without it
//! and have no support period. The date is never read from the network.
//!
//! The startup banner and `config check` name the end date, and say when the
//! release is past it; `OPTIONS {base}/` declares it. Every check takes the
//! day it is judged on as an argument, so a test fixes the clock.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

/// The length of a release's support period, in years: the minimum of
/// Regulation (EU) 2024/2847 Art 13(8), third subparagraph.
pub const SUPPORT_YEARS: i16 = 5;

/// The release date the release lane compiled in, as `YYYY-MM-DD`, or
/// `None` for a build without one.
const RELEASE_DATE: Option<&str> = option_env!("FERROFED_RELEASE_DATE");

/// The year, month and day of [`RELEASE_DATE`], when it is set.
const RELEASED: Option<(i16, i8, i8)> = match RELEASE_DATE {
    None => None,
    Some(text) => released(text.as_bytes()),
};

// NOTE: no specification governs this: our own design; a release date the
// binary cannot read fails the build, so no release ships without its end date.
const _: () = assert!(
    RELEASED.is_some()
        || match RELEASE_DATE {
            None => true,
            Some(text) => text.is_empty(),
        },
    "FERROFED_RELEASE_DATE is set to no YYYY-MM-DD date"
);

/// The last day of the running release's support period, or `None` when the
/// build has none.
///
/// A `FERROFED_RELEASE_DATE` that names no calendar day fails the build.
pub const SUPPORTED_UNTIL: Option<Date> = match RELEASED {
    None => None,
    Some((year, month, day)) => {
        // Constructing the release date refuses a day the calendar lacks.
        let _released: Date = Date::constant(year, month, day);
        Some(end_of(year, month, day))
    }
};

/// The support standing of a build on a given day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// The build has no support period: a development build or a
    /// pre-release.
    NoPeriod,
    /// The release is supported until the date, inclusive, and the day it
    /// is judged on is not past it.
    Until(Date),
    /// The release's support period ended on the date, before the day it
    /// is judged on.
    Ended(Date),
}

impl Support {
    /// Returns the standing of a build supported `until` the date, judged
    /// on `today`.
    #[must_use]
    pub fn at(until: Option<Date>, today: Date) -> Self {
        match until {
            None => Self::NoPeriod,
            Some(end) if today > end => Self::Ended(end),
            Some(end) => Self::Until(end),
        }
    }

    /// Returns the standing of the running build today, in UTC.
    #[must_use]
    pub fn current() -> Self {
        Self::at(SUPPORTED_UNTIL, today())
    }

    /// Returns the notice that release `version` is past its support
    /// period, or `None` while it is within it or has none.
    #[must_use]
    pub fn notice(self, version: &str) -> Option<String> {
        match self {
            Self::Ended(end) => Some(format!(
                "FerroFED v{version} is past its support period, which ended on {end}: \
                 it receives no security fixes, so move to the latest release"
            )),
            Self::NoPeriod | Self::Until(_) => None,
        }
    }
}

/// Returns today's date in UTC, from the system clock.
#[must_use]
pub fn today() -> Date {
    TimeZone::UTC.to_datetime(Timestamp::now()).date()
}

/// Returns the last day of the support period of a release made on `year`,
/// `month` and `day`: the same day [`SUPPORT_YEARS`] years on.
///
/// 29 February becomes 1 March, so the period is never shorter than
/// [`SUPPORT_YEARS`] years, as `scripts/release/changelog.sh` computes it.
const fn end_of(year: i16, month: i8, day: i8) -> Date {
    // NOTE: no specification governs this: our own design; 29 February has
    // no anniversary in a common year, and 1 March keeps the period whole.
    if month == 2 && day == 29 {
        Date::constant(year + SUPPORT_YEARS, 3, 1)
    } else {
        Date::constant(year + SUPPORT_YEARS, month, day)
    }
}

/// Returns the year, month and day of `text` when it is `YYYY-MM-DD` in
/// ASCII digits, or `None` when it is not.
///
/// It reads the shape alone: [`Date::constant`] judges the calendar day.
const fn released(text: &[u8]) -> Option<(i16, i8, i8)> {
    let &[y0, y1, y2, y3, b'-', m0, m1, b'-', d0, d1] = text else {
        return None;
    };
    let (Some(year), Some(month), Some(day)) = (
        number(&[y0, y1, y2, y3]),
        number(&[m0, m1]),
        number(&[d0, d1]),
    ) else {
        return None;
    };
    // Four digits fit an `i16` and two an `i8`, so each reads its low bytes.
    let [_, month] = month.to_be_bytes();
    let [_, day] = day.to_be_bytes();
    Some((
        i16::from_be_bytes(year.to_be_bytes()),
        i8::from_ne_bytes([month]),
        i8::from_ne_bytes([day]),
    ))
}

/// Returns the value of the ASCII digits `text`, at most four of them, or
/// `None` when one is no digit.
const fn number(text: &[u8]) -> Option<u16> {
    let mut value: u16 = 0;
    let mut rest = text;
    while let [byte, tail @ ..] = rest {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from_be_bytes([0, *byte - b'0']);
        rest = tail;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{Support, end_of, released};

    #[test]
    fn the_end_is_the_same_day_five_years_on() {
        assert_eq!(date(2031, 10, 10), end_of(2026, 10, 10));
        assert_eq!(date(2032, 1, 9), end_of(2027, 1, 9));
    }

    #[test]
    fn the_twenty_ninth_of_february_ends_on_the_first_of_march() {
        assert_eq!(date(2033, 3, 1), end_of(2028, 2, 29));
    }

    #[test]
    fn a_release_date_reads_as_its_year_month_and_day() {
        assert_eq!(Some((2026, 10, 5)), released(b"2026-10-05"));
    }

    #[test]
    fn a_release_date_of_another_shape_reads_as_none() {
        for text in [
            "",
            "2026-1-05",
            "2026/10/05",
            "26-10-05",
            "2026-10-05 ",
            "2026-1a-05",
        ] {
            assert_eq!(None, released(text.as_bytes()), "{text:?}");
        }
    }

    #[test]
    fn the_last_day_of_the_period_is_supported() {
        let end = date(2031, 10, 5);
        assert_eq!(
            Support::Until(end),
            Support::at(Some(end), date(2026, 10, 5))
        );
        assert_eq!(Support::Until(end), Support::at(Some(end), end));
        assert_eq!(None, Support::at(Some(end), end).notice("0.0.9"));
    }

    #[test]
    fn the_day_after_the_period_has_ended() {
        let end = date(2031, 10, 5);
        let ended = Support::at(Some(end), date(2031, 10, 6));
        assert_eq!(Support::Ended(end), ended);
        assert_eq!(
            Some(
                "FerroFED v0.0.9 is past its support period, which ended on 2031-10-05: \
                 it receives no security fixes, so move to the latest release"
                    .to_owned()
            ),
            ended.notice("0.0.9")
        );
    }

    #[test]
    fn a_build_without_a_release_date_has_no_period_on_any_day() {
        for today in [date(2000, 1, 1), date(2099, 12, 31)] {
            assert_eq!(Support::NoPeriod, Support::at(None, today));
            assert_eq!(None, Support::NoPeriod.notice("0.0.9"));
        }
    }
}
