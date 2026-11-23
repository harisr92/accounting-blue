//! Money arithmetic shared by every module: one rounding rule and percentages
//!
//! Amounts are [`BigDecimal`] in a currency's major unit (rupees, euros, dollars) with
//! [`MONEY_SCALE`] decimal places once rounded. [`round_money`] is the only rounding rule in the
//! crate; the Indian tax code re-exports it as `gst::tax::round_to_paise`.

use bigdecimal::{BigDecimal, RoundingMode};

/// Decimal places in a rounded amount: one minor unit (a paisa, a cent) is 0.01
pub const MONEY_SCALE: i64 = 2;

/// Percent in a whole
const HUNDRED: i32 = 100;

/// `rate` percent of `amount`, exactly; round the result with [`round_money`]
///
/// # Example
///
/// ```
/// use accounting_core::money::percent_of;
/// use bigdecimal::BigDecimal;
///
/// assert_eq!(percent_of(&BigDecimal::from(1500), &BigDecimal::from(18)), BigDecimal::from(270));
/// ```
#[must_use]
pub fn percent_of(amount: &BigDecimal, rate: &BigDecimal) -> BigDecimal {
    (amount * rate) / BigDecimal::from(HUNDRED)
}

/// Round an amount to the nearest minor unit, with halves rounded away from zero
///
/// 0.025 becomes 0.03 and 0.0249 becomes 0.02. The result always has [`MONEY_SCALE`] decimal
/// places.
///
/// # Example
///
/// ```
/// use accounting_core::money::round_money;
/// use bigdecimal::BigDecimal;
/// use std::str::FromStr;
///
/// let round = |s| round_money(&BigDecimal::from_str(s).unwrap()).to_string();
/// assert_eq!(round("0.02475"), "0.02");
/// assert_eq!(round("0.025"), "0.03");
/// assert_eq!(round("-0.025"), "-0.03");
/// assert_eq!(round("90"), "90.00");
/// ```
#[must_use]
pub fn round_money(amount: &BigDecimal) -> BigDecimal {
    amount.with_scale_round(MONEY_SCALE, RoundingMode::HalfUp)
}
