//! The tax period a return is filed for

use super::gstr1::Gstr1Error;
use chrono::{Datelike, NaiveDate};
use serde::{Serialize, Serializer};
use std::fmt;

/// Earliest year a return period may fall in: GST took effect in India in July 2017
pub const FIRST_GST_YEAR: i32 = 2017;

/// Latest year a return period may fall in, so the portal's four-digit year always fits
const LAST_PERIOD_YEAR: i32 = 9999;

/// One calendar month a return is filed for
///
/// The portal names a period `MMYYYY` (`112024` for November 2024); this type serialises as that
/// string and [`fmt::Display`] prints it the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReturnPeriod {
    year: i32,
    month: u32,
}

impl ReturnPeriod {
    /// The period for a month of a year
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::InvalidPeriod`] if the month is not 1-12 or the year is before
    /// [`FIRST_GST_YEAR`] or after 9999.
    pub fn new(year: i32, month: u32) -> Result<Self, Gstr1Error> {
        if (1..=12).contains(&month) && (FIRST_GST_YEAR..=LAST_PERIOD_YEAR).contains(&year) {
            Ok(Self { year, month })
        } else {
            Err(Gstr1Error::InvalidPeriod { year, month })
        }
    }

    /// The period a date falls in
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::InvalidPeriod`] if the date is outside the years [`ReturnPeriod::new`]
    /// accepts.
    pub fn containing(date: NaiveDate) -> Result<Self, Gstr1Error> {
        Self::new(date.year(), date.month())
    }

    /// Calendar year
    #[must_use]
    pub fn year(&self) -> i32 {
        self.year
    }

    /// Month, 1-12
    #[must_use]
    pub fn month(&self) -> u32 {
        self.month
    }

    /// Whether a date falls in this period
    #[must_use]
    pub fn contains(&self, date: NaiveDate) -> bool {
        date.year() == self.year && date.month() == self.month
    }

    /// The period as the portal writes it: `MMYYYY`
    #[must_use]
    pub fn to_portal_string(&self) -> String {
        format!("{:02}{:04}", self.month, self.year)
    }
}

impl fmt::Display for ReturnPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_portal_string())
    }
}

impl Serialize for ReturnPeriod {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_portal_string())
    }
}
