//! Tax regimes and the breakdown of tax they produce
//!
//! A [`TaxRegime`] says what tax a line pays: given the line's taxable value and its rate, it
//! returns a [`TaxBreakdown`] of named [`TaxComponent`]s. [`NoTax`] charges nothing and
//! [`FlatRate`] charges one tax at the line's rate, such as VAT or a sales tax. The Indian GST
//! regime, which splits a rate into CGST + SGST or charges IGST, lives in `gst::tax` behind the
//! `india` feature.
//!
//! Regimes are pure: they take values and return values, and the same input always gives the
//! same breakdown.

use crate::error::BoxError;
use crate::money::{percent_of, round_money};
use bigdecimal::{BigDecimal, Signed};
use serde::{Deserialize, Serialize};

/// Largest tax rate a line may carry, as a percentage
pub const MAX_TAX_RATE: u32 = 100;

/// One named tax on an amount, such as `VAT` or `CGST`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxComponent {
    /// Short code naming the tax, such as `VAT`, `CGST` or `IGST`; posting matches it to an account
    pub code: String,
    /// Tax charged, rounded to the minor unit
    pub amount: BigDecimal,
}

impl TaxComponent {
    /// A component of `amount` under `code`
    pub fn new(code: impl Into<String>, amount: BigDecimal) -> Self {
        Self {
            code: code.into(),
            amount,
        }
    }
}

/// The taxable value of a line or document, the taxes on it, and the total
///
/// Components keep the order they first appear in. Adding breakdowns merges components with the
/// same code, so the sum of an invoice's lines has one component per tax.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxBreakdown {
    /// Value the tax is charged on
    pub taxable_value: BigDecimal,
    /// Each tax charged, in the order it first appears
    pub components: Vec<TaxComponent>,
    /// Sum of the components
    pub total_tax: BigDecimal,
    /// Taxable value plus total tax
    pub total: BigDecimal,
}

impl TaxBreakdown {
    /// A breakdown of `components` on `taxable_value`, with the totals worked out
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::billing::{TaxBreakdown, TaxComponent};
    /// use bigdecimal::BigDecimal;
    ///
    /// let breakdown = TaxBreakdown::new(
    ///     BigDecimal::from(1000),
    ///     vec![TaxComponent::new("VAT", BigDecimal::from(200))],
    /// );
    /// assert_eq!(breakdown.total, BigDecimal::from(1200));
    /// assert_eq!(breakdown.tax("VAT"), BigDecimal::from(200));
    /// assert_eq!(breakdown.tax("GST"), BigDecimal::from(0));
    /// ```
    #[must_use]
    pub fn new(taxable_value: BigDecimal, components: Vec<TaxComponent>) -> Self {
        let total_tax: BigDecimal = components.iter().map(|c| &c.amount).sum();
        let total = &taxable_value + &total_tax;
        Self {
            taxable_value,
            components,
            total_tax,
            total,
        }
    }

    /// Tax charged under `code`; zero when there is no such component
    #[must_use]
    pub fn tax(&self, code: &str) -> BigDecimal {
        self.components
            .iter()
            .filter(|component| component.code == code)
            .map(|component| &component.amount)
            .sum()
    }

    /// Every amount with its sign flipped, for a document that reduces the supply
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            taxable_value: -&self.taxable_value,
            components: self
                .components
                .iter()
                .map(|c| TaxComponent::new(c.code.clone(), -&c.amount))
                .collect(),
            total_tax: -&self.total_tax,
            total: -&self.total,
        }
    }

    /// This breakdown plus `other`, merging components with the same code
    #[must_use]
    pub fn plus(&self, other: &TaxBreakdown) -> Self {
        let components = other
            .components
            .iter()
            .fold(self.components.clone(), |merged, component| {
                merge_component(merged, component)
            });
        Self::new(&self.taxable_value + &other.taxable_value, components)
    }
}

/// `components` with `component` added to the one of the same code, or appended
fn merge_component(components: Vec<TaxComponent>, component: &TaxComponent) -> Vec<TaxComponent> {
    if components.iter().any(|c| c.code == component.code) {
        components
            .into_iter()
            .map(|c| {
                if c.code == component.code {
                    TaxComponent::new(c.code, c.amount + &component.amount)
                } else {
                    c
                }
            })
            .collect()
    } else {
        components
            .into_iter()
            .chain(std::iter::once(component.clone()))
            .collect()
    }
}

impl<'a> std::iter::Sum<&'a TaxBreakdown> for TaxBreakdown {
    fn sum<I: Iterator<Item = &'a TaxBreakdown>>(iter: I) -> Self {
        iter.fold(Self::default(), |total, line| total.plus(line))
    }
}

/// What tax a line pays
///
/// Implement it to support a tax system the crate doesn't ship. An implementation is pure: the
/// breakdown depends only on the regime's own settings and the arguments.
pub trait TaxRegime {
    /// Why the regime could not tax a line
    type Error: std::error::Error + Send + Sync + 'static;

    /// Tax on a line worth `taxable_value` (before tax, after discounts) at `rate` percent
    ///
    /// The breakdown's `taxable_value` is `taxable_value` rounded to the minor unit, and every
    /// component is rounded too, so the breakdown can be posted as it is.
    ///
    /// # Errors
    ///
    /// The regime's own error, such as a rate it does not allow.
    fn line_tax(
        &self,
        taxable_value: &BigDecimal,
        rate: &BigDecimal,
    ) -> Result<TaxBreakdown, Self::Error>;
}

/// No tax at all: every line's total is its taxable value
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoTax;

impl TaxRegime for NoTax {
    type Error = TaxError;

    fn line_tax(
        &self,
        taxable_value: &BigDecimal,
        _rate: &BigDecimal,
    ) -> Result<TaxBreakdown, TaxError> {
        Ok(TaxBreakdown::new(round_money(taxable_value), Vec::new()))
    }
}

/// One tax at the line's rate, such as VAT or a sales tax
///
/// The taxable value is rounded first and the tax is the rate of that, rounded. A line at a rate
/// of 0 carries a zero component, which posting leaves out.
///
/// # Example
///
/// ```
/// use accounting_core::billing::{FlatRate, TaxRegime};
/// use bigdecimal::BigDecimal;
///
/// # fn main() -> Result<(), accounting_core::billing::TaxError> {
/// let vat = FlatRate::new("VAT");
/// let breakdown = vat.line_tax(&BigDecimal::from(1000), &BigDecimal::from(20))?;
/// assert_eq!(breakdown.tax("VAT"), BigDecimal::from(200));
/// assert_eq!(breakdown.total, BigDecimal::from(1200));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatRate {
    /// Code of the tax, such as `VAT`
    pub code: String,
}

impl FlatRate {
    /// A flat-rate tax under `code`
    pub fn new(code: impl Into<String>) -> Self {
        Self { code: code.into() }
    }
}

impl TaxRegime for FlatRate {
    type Error = TaxError;

    fn line_tax(
        &self,
        taxable_value: &BigDecimal,
        rate: &BigDecimal,
    ) -> Result<TaxBreakdown, TaxError> {
        check_rate(rate)?;
        let base = round_money(taxable_value);
        let tax = round_money(&percent_of(&base, rate));
        Ok(TaxBreakdown::new(
            base,
            vec![TaxComponent::new(self.code.clone(), tax)],
        ))
    }
}

/// Check `rate` is a percentage from 0 to [`MAX_TAX_RATE`]
///
/// # Errors
///
/// [`TaxError::RateOutOfRange`] for any other rate.
pub fn check_rate(rate: &BigDecimal) -> Result<(), TaxError> {
    if rate.is_negative() || *rate > MAX_TAX_RATE {
        Err(TaxError::RateOutOfRange(rate.clone()))
    } else {
        Ok(())
    }
}

/// Why a regime could not tax a line
#[derive(Debug, thiserror::Error)]
pub enum TaxError {
    /// The rate is outside 0-100
    #[error("tax rate must be between 0 and {MAX_TAX_RATE}, got {0}")]
    RateOutOfRange(BigDecimal),
    /// A regime outside this crate failed; carries its own error
    #[error("tax regime failed: {0}")]
    Regime(#[source] BoxError),
}
