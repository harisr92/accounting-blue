//! Lines and documents, and pricing them under a tax regime
//!
//! A [`BillableLine`] is anything priced as quantity × unit price less a [`Discount`], at a tax
//! rate. A [`TaxDocument`] is a numbered, dated list of such lines with an optional discount off
//! the subtotal, taxed under a [`TaxRegime`]. [`price_lines`] is the one pricing engine every
//! document shares: it rounds each line, shares the document discount across the lines by value
//! with [`apportion`], and asks the regime for the tax on what is left.
//!
//! [`LineItem`] is the plain line this module ships. The Indian `GstLineItem` is another
//! [`BillableLine`] that also carries an HSN/SAC code.

use crate::billing::discount::{apportion, Discount, DiscountError};
use crate::billing::tax::{check_rate, TaxBreakdown, TaxRegime, MAX_TAX_RATE};
use crate::money::round_money;
use bigdecimal::{BigDecimal, Signed, Zero};
use chrono::NaiveDate;
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
    /// [`TaxDocument::line_tax_breakdowns`] takes that share off and rounds, so its
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
    /// use accounting_core::billing::{BillableLine, Discount, LineItem};
    /// use bigdecimal::BigDecimal;
    ///
    /// # fn main() -> Result<(), accounting_core::billing::LineError> {
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

/// A numbered, dated document of billable lines, taxed under one regime
///
/// The provided methods hold the arithmetic every document shares: the subtotal, the discount
/// off it, and the tax breakdown of each line and of the whole document.
pub trait TaxDocument {
    /// The document's line type
    type Line: BillableLine;

    /// The regime that taxes the document's lines
    type Regime: TaxRegime;

    /// Why the document could not be priced: a discount that no longer fits, or the regime's
    /// own error
    type Error: From<DiscountError> + From<<Self::Regime as TaxRegime>::Error>;

    /// Document number
    fn number(&self) -> &str;

    /// Date of issue
    fn date(&self) -> NaiveDate;

    /// The document's lines
    fn lines(&self) -> &[Self::Line];

    /// Discount off the subtotal, before tax; none unless the document gives one
    fn document_discount(&self) -> Option<&Discount> {
        None
    }

    /// The regime the document's lines are taxed under
    fn regime(&self) -> Self::Regime;

    /// Sum of the lines' taxable values after their own discounts, each rounded, before the
    /// [`TaxDocument::document_discount`]
    fn subtotal(&self) -> BigDecimal {
        line_values(self.lines()).iter().sum()
    }

    /// Amount the [`TaxDocument::document_discount`] takes off the subtotal; zero when there is
    /// none
    fn discount_amount(&self) -> BigDecimal {
        self.document_discount()
            .map_or_else(BigDecimal::zero, |discount| {
                discount.amount_off(&self.subtotal())
            })
    }

    /// Check the [`TaxDocument::document_discount`] can come off the subtotal
    ///
    /// # Errors
    ///
    /// The [`DiscountError`] naming the rule it breaks.
    fn check_discount(&self) -> Result<(), DiscountError> {
        self.document_discount()
            .map_or(Ok(()), |discount| discount.check(&self.subtotal()))
    }

    /// Tax breakdown for each line, in order, after its share of the document discount
    ///
    /// # Errors
    ///
    /// A [`DiscountError`] if the document discount no longer fits the subtotal, such as after
    /// its lines were edited, or the regime's error for the first line it can't tax.
    fn line_tax_breakdowns(&self) -> Result<Vec<TaxBreakdown>, Self::Error> {
        Ok(price_lines(self)?.breakdowns)
    }

    /// Tax breakdown summed across all lines, one component per tax
    ///
    /// # Errors
    ///
    /// As [`TaxDocument::line_tax_breakdowns`].
    fn tax_breakdown(&self) -> Result<TaxBreakdown, Self::Error> {
        Ok(self.line_tax_breakdowns()?.iter().sum())
    }
}

/// Each line's taxable value after its own discount, rounded
#[must_use]
pub fn line_values<L: BillableLine>(lines: &[L]) -> Vec<BigDecimal> {
    lines
        .iter()
        .map(|line| round_money(&line.taxable_value()))
        .collect()
}

/// A document's lines priced once: the subtotal, the discount off it, and each line's breakdown
/// after its share of that discount
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedLines {
    /// Sum of the lines after their own discounts, before the document discount
    pub subtotal: BigDecimal,
    /// Amount the document discount takes off the subtotal; zero when there is none
    pub discount: BigDecimal,
    /// Each line's breakdown, in order
    pub breakdowns: Vec<TaxBreakdown>,
}

/// Price `document`'s lines: share its discount across them by value and tax what is left
///
/// The breakdown methods of [`TaxDocument`] and any print model read this, so the line values and
/// the subtotal are worked out once.
///
/// # Errors
///
/// As [`TaxDocument::line_tax_breakdowns`].
pub fn price_lines<D: TaxDocument + ?Sized>(document: &D) -> Result<PricedLines, D::Error> {
    let values = line_values(document.lines());
    let subtotal: BigDecimal = values.iter().sum();
    let discount = document
        .document_discount()
        .map_or(Ok(BigDecimal::zero()), |discount| {
            discount
                .check(&subtotal)
                .map(|()| discount.amount_off(&subtotal))
        })?;
    let regime = document.regime();
    let breakdowns = document
        .lines()
        .iter()
        .zip(values.iter().zip(apportion(&discount, &values)))
        .map(|(line, (value, share))| regime.line_tax(&(value - share), line.tax_rate()))
        .collect::<Result<_, _>>()?;
    Ok(PricedLines {
        subtotal,
        discount,
        breakdowns,
    })
}
