//! Creation timestamp recorded in a SAV file header.

use crate::spss::sav::sav_timestamp::SavTimestamp;

/// Creation timestamp recorded in a SAV file header.
///
/// Parsing is all-or-nothing: if every component (date and time) was
/// recognized, the result is [`Parsed`](Self::Parsed); if any component
/// failed to parse, the original raw strings are preserved verbatim
/// in [`Unparsed`](Self::Unparsed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SavCreationTimestamp {
    /// Both date and time parsed successfully.
    Parsed(SavTimestamp),
    /// At least one part was unparseable; the raw strings are
    /// preserved.
    #[non_exhaustive]
    Unparsed {
        /// Raw date string from the header.
        date: String,
        /// Raw time string from the header.
        time: String,
    },
}

impl SavCreationTimestamp {
    /// Constructs a [`SavCreationTimestamp`] from the 9-byte
    /// `creation_date` (`"DD MMM YY"`) and 8-byte `creation_time`
    /// (`"HH:MM:SS"`) fields of a SAV file header. Falls back to
    /// [`Unparsed`](Self::Unparsed) when either component is
    /// unrecognized.
    #[must_use]
    pub(crate) fn from_header_bytes(date_bytes: [u8; 9], time_bytes: [u8; 8]) -> Self {
        let date = ascii_str(&date_bytes);
        let time = ascii_str(&time_bytes);

        let parsed_date = parse_date(&date);
        let parsed_time = parse_time(&time);
        if let Some((day, month, year)) = parsed_date
            && let Some((hour, minute, second)) = parsed_time
        {
            let timestamp = SavTimestamp::builder()
                .day(day)
                .month(month)
                .year(year)
                .hour(hour)
                .minute(minute)
                .second(second)
                .build();
            Self::Parsed(timestamp)
        } else {
            Self::Unparsed { date, time }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Best-effort ASCII rendering of a fixed-width string field.
/// Bytes outside printable ASCII become `'?'`; trailing spaces and
/// NULs are trimmed.
fn ascii_str(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map_or(0, |p| p + 1);
    bytes[..end]
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                '?'
            }
        })
        .collect()
}

fn parse_date(s: &str) -> Option<(u8, u8, u8)> {
    let mut parts = s.split_whitespace();
    let day = parts.next()?.parse::<u8>().ok()?;
    let month = parse_month(parts.next()?)?;
    let year = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((day, month, year))
}

fn parse_month(s: &str) -> Option<u8> {
    let trimmed = s.trim();
    if trimmed.len() != 3 {
        return None;
    }
    let mut buf = [0u8; 3];
    buf.copy_from_slice(trimmed.as_bytes());
    for b in &mut buf {
        b.make_ascii_lowercase();
    }
    match &buf {
        b"jan" => Some(1),
        b"feb" => Some(2),
        b"mar" => Some(3),
        b"apr" => Some(4),
        b"may" => Some(5),
        b"jun" => Some(6),
        b"jul" => Some(7),
        b"aug" => Some(8),
        b"sep" => Some(9),
        b"oct" => Some(10),
        b"nov" => Some(11),
        b"dec" => Some(12),
        _ => None,
    }
}

fn parse_time(s: &str) -> Option<(u8, u8, u8)> {
    let mut parts = s.trim().split(':');
    let hour = parts.next()?.parse::<u8>().ok()?;
    let minute = parts.next()?.parse::<u8>().ok()?;
    let second = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((hour, minute, second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_date_and_time() {
        let ts = SavCreationTimestamp::from_header_bytes(*b"01 Jan 24", *b"13:45:30");
        match ts {
            SavCreationTimestamp::Parsed(parsed) => {
                assert_eq!(parsed.day(), 1);
                assert_eq!(parsed.month(), 1);
                assert_eq!(parsed.year(), 24);
                assert_eq!(parsed.hour(), 13);
                assert_eq!(parsed.minute(), 45);
                assert_eq!(parsed.second(), 30);
            }
            other @ SavCreationTimestamp::Unparsed { .. } => {
                panic!("expected Parsed, got {other:?}")
            }
        }
    }

    #[test]
    fn case_insensitive_month() {
        let ts = SavCreationTimestamp::from_header_bytes(*b"15 mar 99", *b"00:00:00");
        match ts {
            SavCreationTimestamp::Parsed(parsed) => assert_eq!(parsed.month(), 3),
            other @ SavCreationTimestamp::Unparsed { .. } => {
                panic!("expected Parsed, got {other:?}")
            }
        }
    }

    #[test]
    fn unparseable_date_falls_back_to_raw_strings() {
        let ts = SavCreationTimestamp::from_header_bytes(*b"garbage  ", *b"13:45:30");
        match ts {
            SavCreationTimestamp::Unparsed { date, time } => {
                assert!(date.starts_with("garbage"));
                assert_eq!(time, "13:45:30");
            }
            other @ SavCreationTimestamp::Parsed(_) => {
                panic!("expected Unparsed, got {other:?}")
            }
        }
    }

    #[test]
    fn unparseable_time_falls_back() {
        let ts = SavCreationTimestamp::from_header_bytes(*b"01 Jan 24", *b"hh:mm:ss");
        assert!(matches!(ts, SavCreationTimestamp::Unparsed { .. }));
    }

    #[test]
    fn trailing_spaces_in_date_are_tolerated() {
        let ts = SavCreationTimestamp::from_header_bytes(*b"1 Jan 24 ", *b"13:45:30");
        assert!(matches!(ts, SavCreationTimestamp::Parsed(_)));
    }

    #[test]
    fn ascii_str_replaces_non_printable_bytes() {
        let s = ascii_str(&[b'A', 0x01, b'B', 0x80, b' ']);
        assert_eq!(s, "A?B?");
    }
}
