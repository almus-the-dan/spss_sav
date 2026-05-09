//! A SAV creation timestamp.

/// The components of a successfully parsed SAV creation timestamp.
///
/// All fields are stored as the raw on-disk values — `year` is the
/// two-digit value (0–99) before any base-year is applied, and no
/// calendar validation is performed at construction time. Use the
/// chrono adapter (gated on the `chrono` feature) to get a
/// validated `NaiveDateTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SavTimestamp {
    day: u8,
    month: u8,
    year: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl SavTimestamp {
    /// Returns a fresh [`SavTimestampBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavTimestampBuilder {
        SavTimestampBuilder::default()
    }

    /// Day of the month as written on disk (typically `1..=31`).
    #[must_use]
    #[inline]
    pub fn day(&self) -> u8 {
        self.day
    }

    /// Month of the year as written on disk (typically `1..=12`).
    #[must_use]
    #[inline]
    pub fn month(&self) -> u8 {
        self.month
    }

    /// Two-digit year as written on disk (`0..=99`). No base-year
    /// is applied.
    #[must_use]
    #[inline]
    pub fn year(&self) -> u8 {
        self.year
    }

    /// Hour as written on disk (typically `0..=23`).
    #[must_use]
    #[inline]
    pub fn hour(&self) -> u8 {
        self.hour
    }

    /// Minute as written on disk (typically `0..=59`).
    #[must_use]
    #[inline]
    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// Second as written on disk (typically `0..=59`).
    #[must_use]
    #[inline]
    pub fn second(&self) -> u8 {
        self.second
    }
}

/// Builder for [`SavTimestamp`].
///
/// Unset components default to `0`. No range validation is
/// performed at [`build`](Self::build) time — out-of-range values
/// (e.g. `month = 13`) round-trip verbatim, matching the
/// no-validation policy of the parent type.
#[derive(Debug, Default, Clone)]
pub struct SavTimestampBuilder {
    day: u8,
    month: u8,
    year: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl SavTimestampBuilder {
    /// Sets the day of the month.
    #[must_use]
    #[inline]
    pub fn day(mut self, day: u8) -> Self {
        self.day = day;
        self
    }

    /// Sets the month of the year.
    #[must_use]
    #[inline]
    pub fn month(mut self, month: u8) -> Self {
        self.month = month;
        self
    }

    /// Sets the two-digit year (`0..=99`).
    #[must_use]
    #[inline]
    pub fn year(mut self, year: u8) -> Self {
        self.year = year;
        self
    }

    /// Sets the hour.
    #[must_use]
    #[inline]
    pub fn hour(mut self, hour: u8) -> Self {
        self.hour = hour;
        self
    }

    /// Sets the minute.
    #[must_use]
    #[inline]
    pub fn minute(mut self, minute: u8) -> Self {
        self.minute = minute;
        self
    }

    /// Sets the second.
    #[must_use]
    #[inline]
    pub fn second(mut self, second: u8) -> Self {
        self.second = second;
        self
    }

    /// Finalizes this builder into a [`SavTimestamp`].
    #[must_use]
    #[inline]
    pub fn build(self) -> SavTimestamp {
        SavTimestamp {
            day: self.day,
            month: self.month,
            year: self.year,
            hour: self.hour,
            minute: self.minute,
            second: self.second,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_round_trips_each_field() {
        let ts = SavTimestamp::builder()
            .day(15)
            .month(3)
            .year(99)
            .hour(23)
            .minute(45)
            .second(7)
            .build();
        assert_eq!(ts.day(), 15);
        assert_eq!(ts.month(), 3);
        assert_eq!(ts.year(), 99);
        assert_eq!(ts.hour(), 23);
        assert_eq!(ts.minute(), 45);
        assert_eq!(ts.second(), 7);
    }

    #[test]
    fn builder_defaults_to_zeros() {
        let ts = SavTimestamp::builder().build();
        assert_eq!(ts.day(), 0);
        assert_eq!(ts.month(), 0);
        assert_eq!(ts.year(), 0);
        assert_eq!(ts.hour(), 0);
        assert_eq!(ts.minute(), 0);
        assert_eq!(ts.second(), 0);
    }
}
