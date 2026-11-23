//! Pricing a document's lines under a tax regime
//!
//! A [`TaxDocument`] is a numbered, dated list of lines with an optional discount off the
//! subtotal, taxed under a [`TaxRegime`]. [`price_lines`] is the one pricing engine every
//! document shares: it rounds each line, shares the document discount across the lines by value
//! with [`apportion`], and asks the regime for the tax on what is left.

use crate::document::discount::{apportion, Discount, DiscountError};
use crate::document::line::BillableLine;
use crate::document::tax::{TaxBreakdown, TaxRegime};
use crate::money::round_money;
use bigdecimal::{BigDecimal, Zero};
use chrono::NaiveDate;

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
