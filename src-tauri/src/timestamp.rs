use std::{
    fmt::Write,
    time::{SystemTime, UNIX_EPOCH},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Clone, Copy)]
pub(crate) struct Timestamp {
    date_time: OffsetDateTime,
    leap_second: bool,
}

impl Timestamp {
    pub(crate) fn now_utc() -> Self {
        Self {
            date_time: OffsetDateTime::now_utc(),
            leap_second: false,
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        if !matches!(value.as_bytes().get(10), Some(b'T' | b't' | b' ')) {
            return None;
        }

        // Keep the former parser's leap-second spelling and subsecond order.
        // OffsetDateTime's RFC 3339 parser collapses leap seconds to a stand-in.
        let leap_second = value.as_bytes().get(17..19) == Some(b"60");
        let date_time = if leap_second {
            let ordinary_second = format!("{}59{}", value.get(..17)?, value.get(19..)?);
            OffsetDateTime::parse(&ordinary_second, &Rfc3339).ok()?
        } else {
            OffsetDateTime::parse(value, &Rfc3339).ok()?
        };
        Some(Self {
            date_time,
            leap_second,
        })
    }

    pub(crate) fn instant(self) -> (i64, bool, u32) {
        (
            self.date_time.unix_timestamp(),
            self.leap_second,
            self.date_time.nanosecond(),
        )
    }

    pub(crate) fn format(self) -> String {
        let (year, month, day) = self.date_time.date().to_calendar_date();
        let year = if (0..=9999).contains(&year) {
            format!("{year:04}")
        } else {
            format!("{year:+05}")
        };
        let (hour, minute, second, nanos) = self.date_time.time().as_hms_nano();
        let second = second + u8::from(self.leap_second);
        let month = u8::from(month);
        let mut text = format!("{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");

        // Persist the existing wire format. SQLite orders UTC timestamps as text,
        // so a "Z" suffix or a different fraction width would mix poorly with old rows.
        if nanos != 0 {
            let (fraction, width) = if nanos.is_multiple_of(1_000_000) {
                (nanos / 1_000_000, 3)
            } else if nanos.is_multiple_of(1_000) {
                (nanos / 1_000, 6)
            } else {
                (nanos, 9)
            };
            write!(text, ".{fraction:0width$}").expect("writing to a String cannot fail");
        }
        let offset = self.date_time.offset().whole_minutes();
        let sign = if offset < 0 { '-' } else { '+' };
        let minutes = offset.unsigned_abs();
        write!(text, "{sign}{:02}:{:02}", minutes / 60, minutes % 60)
            .expect("writing to a String cannot fail");
        text
    }
}

pub(crate) fn now() -> String {
    Timestamp::now_utc().format()
}

pub(crate) fn from_system_time(value: SystemTime) -> Option<String> {
    let nanos = match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(error) => -(error.duration().as_nanos() as i128),
    };
    Some(
        Timestamp {
            date_time: OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?,
            leap_second: false,
        }
        .format(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_offset_and_fraction_spelling_from_previous_versions() {
        for (input, expected) in [
            ("2026-11-22T12:34:56Z", "2026-11-22T12:34:56+00:00"),
            ("2026-11-22T12:34:56.12Z", "2026-11-22T12:34:56.120+00:00"),
            (
                "2026-11-22t12:34:56.123400z",
                "2026-11-22T12:34:56.123400+00:00",
            ),
            (
                "2026-11-22 12:34:56.1234567-00:30",
                "2026-11-22T12:34:56.123456700-00:30",
            ),
            (
                "2026-11-22T12:34:56.000000001+05:30",
                "2026-11-22T12:34:56.000000001+05:30",
            ),
            ("2016-12-31T23:59:60.1Z", "2016-12-31T23:59:60.100+00:00"),
        ] {
            assert_eq!(Timestamp::parse(input).unwrap().format(), expected);
        }
        assert!(Timestamp::parse("2026-11-22X12:34:56Z").is_none());
        assert!(Timestamp::parse("2026-11-31T12:34:56Z").is_none());
        assert!(Timestamp::parse("2026-11-22T12:34:56+24:00").is_none());
    }

    #[test]
    fn compares_instants_across_offsets_and_leap_seconds() {
        let key = |value| Timestamp::parse(value).unwrap().instant();
        assert_eq!(
            key("2016-12-31T23:59:60.1Z"),
            key("2017-01-01T05:29:60.1+05:30")
        );
        assert!(key("2016-12-31T23:59:59.999999999Z") < key("2016-12-31T23:59:60Z"));
        assert!(key("2016-12-31T23:59:60.999999999Z") < key("2017-01-01T00:00:00Z"));
    }

    #[test]
    fn file_times_preserve_fractional_precision_on_either_side_of_epoch() {
        let tick = std::time::Duration::from_nanos(100);
        assert_eq!(
            from_system_time(UNIX_EPOCH + tick).as_deref(),
            Some("1970-01-01T00:00:00.000000100+00:00")
        );
        assert_eq!(
            from_system_time(UNIX_EPOCH - tick).as_deref(),
            Some("1969-12-31T23:59:59.999999900+00:00")
        );
    }
}
