//! Document lines: what is supplied, how many, at what price and tax rate
//!
//! A [`BillableLine`] is anything priced as quantity × unit price less a [`Discount`], at a tax
//! rate. [`LineItem`] is the plain line this module ships; the Indian `GstLineItem` is another
//! [`BillableLine`] that also carries an HSN/SAC code. [`check_line`] holds the rules every line
//! must pass.

use crate::document::discount::{Discount, DiscountError};
use crate::document::tax::{check_rate, MAX_TAX_RATE};
use bigdecimal::{BigDecimal, Signed, Zero};
use serde::{Deserialize, Serialize};

/// A line priced as quantity × unit price, less its own discount, at a tax rate
pub trait BillableLine {
    /// What is supplied
    fn description(&self) -> &str;

    /// Quantity supplied
    fn quantity(&self) -> &BigDecimal;

    /// Price of one unit, before tax
    fn unit_price(&self) -> &BigDecimal;

    /// Discount off quantity × unit price, before tax
    fn discount(&self) -> Option<&Discount>;

    /// Tax rate as a percentage, such as 18 for 18%
    fn tax_rate(&self) -> &BigDecimal;

    /// Value of the line before its discount (quantity × unit price), exactly
    fn gross_value(&self) -> BigDecimal {
        self.quantity() * self.unit_price()
    }

    /// Amount the line's discount takes off its [`BillableLine::gross_value`], rounded to the
    /// minor unit; zero when it has none
    fn discount_amount(&self) -> BigDecimal {
        self.discount().map_or_else(BigDecimal::zero, |discount| {
            discount.amount_off(&self.gross_value())
        })
    }

    /// Taxable value of the line: quantity × unit price less the line's discount, exactly
    ///
    /// This is not rounded, and is before any share of a document discount;
    /// [`TaxDocument::line_tax_breakdowns`](crate::document::TaxDocument::line_tax_breakdowns) takes that share off and rounds, so its
    /// `taxable_value` is the amount that is invoiced and posted.
    fn taxable_value(&self) -> BigDecimal {
        self.gross_value() - self.discount_amount()
    }
}

/// A plain invoice line: a description, quantity, unit price, tax rate and optional discount
///
/// Deserialising goes through the same validation as [`LineItem::new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawLineItem")]
pub struct LineItem {
    /// What is supplied
    pub description: String,
    /// Quantity supplied
    pub quantity: BigDecimal,
    /// Price of one unit, before tax
    pub unit_price: BigDecimal,
    /// Tax rate as a percentage; how it is charged depends on the document's regime
    pub tax_rate: BigDecimal,
    /// Discount off quantity × unit price, before tax
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discount: Option<Discount>,
    /// Unit the quantity is counted in, such as `hours` or `kg`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Unvalidated shape of a [`LineItem`], used when deserialising
#[derive(Deserialize)]
struct RawLineItem {
    description: String,
    quantity: BigDecimal,
    unit_price: BigDecimal,
    tax_rate: BigDecimal,
    #[serde(default)]
    discount: Option<Discount>,
    #[serde(default)]
    unit: Option<String>,
}

impl TryFrom<RawLineItem> for LineItem {
    type Error = LineError;

    fn try_from(raw: RawLineItem) -> Result<Self, Self::Error> {
        let item = Self {
            description: raw.description,
            quantity: raw.quantity,
            unit_price: raw.unit_price,
            tax_rate: raw.tax_rate,
            discount: raw.discount,
            unit: None,
        };
        item.check()?;
        Ok(match raw.unit {
            Some(unit) => item.with_unit(unit),
            None => item,
        })
    }
}

impl LineItem {
    /// Create a validated line
    ///
    /// # Errors
    ///
    /// The [`LineError`] naming the first rule a field breaks.
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::document::{BillableLine, Discount, LineItem};
    /// use bigdecimal::BigDecimal;
    ///
    /// # fn main() -> Result<(), accounting_core::document::LineError> {
    /// let line = LineItem::new("Consulting", BigDecimal::from(10), BigDecimal::from(150),
    ///     BigDecimal::from(20))?
    ///     .with_discount(Discount::Percent(BigDecimal::from(10)))?
    ///     .with_unit("hours");
    /// assert_eq!(line.taxable_value(), BigDecimal::from(1350));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        description: impl Into<String>,
        quantity: BigDecimal,
        unit_price: BigDecimal,
        tax_rate: BigDecimal,
    ) -> Result<Self, LineError> {
        let item = Self {
            description: description.into(),
            quantity,
            unit_price,
            tax_rate,
            discount: None,
            unit: None,
        };
        item.check()?;
        Ok(item)
    }

    /// The line with `discount` taken off its quantity × unit price, before tax
    ///
    /// # Errors
    ///
    /// [`LineError::InvalidDiscount`] if the discount is out of range or more than the line's
    /// [`BillableLine::gross_value`].
    pub fn with_discount(self, discount: Discount) -> Result<Self, LineError> {
        let item = Self {
            discount: Some(discount),
            ..self
        };
        item.check()?;
        Ok(item)
    }

    /// The line with its quantity counted in `unit`; a blank unit leaves it without one
    #[must_use]
    pub fn with_unit(self, unit: impl Into<String>) -> Self {
        let unit = unit.into().trim().to_string();
        Self {
            unit: (!unit.is_empty()).then_some(unit),
            ..self
        }
    }

    /// The field rules, then the discount
    fn check(&self) -> Result<(), LineError> {
        check_line(self)
    }
}

impl BillableLine for LineItem {
    fn description(&self) -> &str {
        &self.description
    }

    fn quantity(&self) -> &BigDecimal {
        &self.quantity
    }

    fn unit_price(&self) -> &BigDecimal {
        &self.unit_price
    }

    fn discount(&self) -> Option<&Discount> {
        self.discount.as_ref()
    }

    fn tax_rate(&self) -> &BigDecimal {
        &self.tax_rate
    }
}

/// Check a line's description, quantity, unit price and rate, then its discount
///
/// # Errors
///
/// As [`check_line_fields`], then [`LineError::InvalidDiscount`] for a discount out of range or
/// more than the line's [`BillableLine::gross_value`].
pub fn check_line<L: BillableLine + ?Sized>(line: &L) -> Result<(), LineError> {
    check_line_fields(line)?;
    line.discount().map_or(Ok(()), |discount| {
        discount
            .check(&line.gross_value())
            .map_err(LineError::InvalidDiscount)
    })
}

/// Check a line's description, quantity, unit price and rate, leaving out its discount
///
/// # Errors
///
/// The [`LineError`] naming the first rule broken: an empty description, a quantity that is not
/// positive, a negative unit price, or a rate outside 0-100.
pub fn check_line_fields<L: BillableLine + ?Sized>(line: &L) -> Result<(), LineError> {
    if line.description().trim().is_empty() {
        Err(LineError::EmptyDescription)
    } else if !line.quantity().is_positive() {
        Err(LineError::NonPositiveQuantity)
    } else if line.unit_price().is_negative() {
        Err(LineError::NegativeUnitPrice)
    } else {
        check_rate(line.tax_rate()).map_err(|_| LineError::RateOutOfRange(line.tax_rate().clone()))
    }
}

/// Why a line was rejected
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LineError {
    /// The description is empty or only whitespace
    #[error("description cannot be empty")]
    EmptyDescription,
    /// The quantity is zero or negative
    #[error("quantity must be positive")]
    NonPositiveQuantity,
    /// The unit price is negative
    #[error("unit price cannot be negative")]
    NegativeUnitPrice,
    /// The tax rate is outside 0-100
    #[error("tax rate must be between 0 and {MAX_TAX_RATE}, got {0}")]
    RateOutOfRange(BigDecimal),
    /// The line's discount is out of range or more than its value
    #[error("{0}")]
    InvalidDiscount(DiscountError),
}
