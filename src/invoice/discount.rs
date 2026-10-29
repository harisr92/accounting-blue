//! Discounts on a line or on an invoice's subtotal
//!
//! A [`Discount`] is a percentage or a flat amount of rupees. A line's discount comes off its
//! quantity times unit price; an invoice's discount comes off its subtotal, the sum of its lines
//! after their own discounts. Both are given before tax, so they reduce the taxable value
//! (Section 15(3) of the CGST Act). An invoice's lines may carry different rates, so its discount
//! is shared across them in proportion to their value by [`apportion`], and each line is taxed on
//! what is left.

use crate::tax::gst::{percent_of, round_to_paise};
use bigdecimal::{BigDecimal, Signed, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Largest percentage a discount may take off
const MAX_DISCOUNT_PERCENT: u32 = 100;

/// A discount given before tax: a percentage of the value it applies to, or a flat amount
///
/// Serialises as `{"percent": "10"}` or `{"amount": "500"}`.
///
/// # Example
///
/// ```
/// use accounting_core::invoice::Discount;
/// use bigdecimal::BigDecimal;
///
/// let base = BigDecimal::from(1999);
/// assert_eq!(Discount::Percent(BigDecimal::from(10)).amount_off(&base).to_string(), "199.90");
/// assert_eq!(Discount::Amount(BigDecimal::from(99)).amount_off(&base).to_string(), "99.00");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Discount {
    /// A percentage of the value, from 0 to 100
    Percent(BigDecimal),
    /// A flat amount of rupees, at most the value
    Amount(BigDecimal),
}

impl Discount {
    /// The rupees this discount takes off `base`, rounded to paise
    ///
    /// Never more than `base`: rounding a discount of the whole of a base with a fraction of a
    /// paisa, such as 100% of 3.335, would otherwise leave a negative value.
    #[must_use]
    pub fn amount_off(&self, base: &BigDecimal) -> BigDecimal {
        let off = match self {
            Self::Percent(percent) => round_to_paise(&percent_of(base, percent)),
            Self::Amount(amount) => round_to_paise(amount),
        };
        if off > *base && !base.is_negative() {
            base.clone()
        } else {
            off
        }
    }

    /// Check the discount can apply to `base`
    ///
    /// # Errors
    ///
    /// [`DiscountError::PercentOutOfRange`] for a percentage outside 0-100,
    /// [`DiscountError::NegativeAmount`] for an amount below zero, or
    /// [`DiscountError::ExceedsValue`] for an amount more than `base`.
    pub fn check(&self, base: &BigDecimal) -> Result<(), DiscountError> {
        match self {
            Self::Percent(percent) if percent.is_negative() || *percent > MAX_DISCOUNT_PERCENT => {
                Err(DiscountError::PercentOutOfRange(percent.clone()))
            }
            Self::Amount(amount) if amount.is_negative() => {
                Err(DiscountError::NegativeAmount(amount.clone()))
            }
            Self::Amount(amount) if amount > base => Err(DiscountError::ExceedsValue {
                discount: amount.clone(),
                value: base.clone(),
            }),
            Self::Percent(_) | Self::Amount(_) => Ok(()),
        }
    }
}

impl fmt::Display for Discount {
    /// `10%` for a percentage, or the plain amount
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Percent(percent) => write!(f, "{}%", percent.normalized().to_plain_string()),
            Self::Amount(amount) => f.write_str(&amount.normalized().to_plain_string()),
        }
    }
}

/// Why a discount was rejected
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DiscountError {
    /// The percentage is outside 0-100
    #[error("discount percentage must be between 0 and {MAX_DISCOUNT_PERCENT}, got {0}")]
    PercentOutOfRange(BigDecimal),
    /// The amount is below zero
    #[error("discount amount cannot be negative, got {0}")]
    NegativeAmount(BigDecimal),
    /// The amount is more than the value it comes off
    #[error("discount {discount} is more than the value {value}")]
    ExceedsValue {
        /// The discount amount
        discount: BigDecimal,
        /// The value it was to come off
        value: BigDecimal,
    },
}

/// Share `total` across `bases` in proportion to each, in paise
///
/// Each share is rounded to paise, and what rounding leaves over goes to the largest base, so the
/// shares always add up to `total` when it is in paise. When the bases sum to zero, every share
/// is zero.
///
/// # Example
///
/// ```
/// use accounting_core::invoice::apportion;
/// use bigdecimal::BigDecimal;
///
/// let bases = [BigDecimal::from(1), BigDecimal::from(1), BigDecimal::from(1)];
/// let shares = apportion(&BigDecimal::from(100), &bases);
/// let shares: Vec<String> = shares.iter().map(ToString::to_string).collect();
/// assert_eq!(shares, ["33.33", "33.33", "33.34"]);
/// ```
#[must_use]
pub fn apportion(total: &BigDecimal, bases: &[BigDecimal]) -> Vec<BigDecimal> {
    let sum: BigDecimal = bases.iter().sum();
    if sum.is_zero() {
        return bases.iter().map(|_| BigDecimal::zero()).collect();
    }
    let shares: Vec<BigDecimal> = bases
        .iter()
        .map(|base| round_to_paise(&(total * base / &sum)))
        .collect();
    let remainder = total - shares.iter().sum::<BigDecimal>();
    let largest = bases
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(index, _)| index);
    shares
        .into_iter()
        .enumerate()
        .map(|(index, share)| {
            if Some(index) == largest {
                share + &remainder
            } else {
                share
            }
        })
        .collect()
}
