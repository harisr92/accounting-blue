//! Sales invoices under any tax regime
//!
//! An [`Invoice`] is a numbered, dated bill to a customer: [`LineItem`]s, an optional discount off
//! the subtotal, and the [`TaxRegime`] its lines are taxed under. Its pricing comes from
//! [`Document`], and [`Invoice::to_entries`] posts it to the ledger.

use crate::document::discount::Discount;
use crate::document::error::DocumentError;
use crate::document::line::LineItem;
use crate::document::posting::PostingLeg;
use crate::document::pricing::Document;
use crate::document::tax::{TaxError, TaxRegime};
use crate::sales::posting::{sales_entries, SalesAccounts};
use crate::types::Entry;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// An invoice taxed under the regime `R`, such as [`FlatRate`](crate::document::FlatRate) for VAT
///
/// Deserialising goes through the same checks as [`Invoice::new`] and
/// [`Invoice::with_discount`].
///
/// # Example
///
/// ```
/// use accounting_core::document::{Document, FlatRate, LineItem};
/// use accounting_core::sales::{Invoice, SalesAccounts};
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// # fn main() -> Result<(), accounting_core::Error> {
/// let line = LineItem::new("Consulting", BigDecimal::from(10), BigDecimal::from(100),
///     BigDecimal::from(20))?;
/// let invoice = Invoice::new("INV-001", NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
///     "Acme Ltd", vec![line], FlatRate::new("VAT"))?;
/// assert_eq!(invoice.tax_breakdown()?.total, BigDecimal::from(1200));
///
/// let accounts = SalesAccounts::new("receivable", "sales").with_output_tax("VAT", "vat-payable");
/// let entries = invoice.to_entries(&accounts)?;
/// assert_eq!(entries.len(), 3);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "RawInvoice<R>",
    bound(deserialize = "R: TaxRegime<Error = TaxError> + Clone + Deserialize<'de>")
)]
pub struct Invoice<R> {
    /// Invoice number
    pub number: String,
    /// Date of issue
    pub date: NaiveDate,
    /// Who is billed
    pub customer: String,
    /// What is billed
    pub lines: Vec<LineItem>,
    /// Discount off the subtotal, before tax, shared across the lines by value
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discount: Option<Discount>,
    /// The regime the lines are taxed under
    pub regime: R,
}

/// Unvalidated shape of an [`Invoice`], used when deserialising
#[derive(Deserialize)]
struct RawInvoice<R> {
    number: String,
    date: NaiveDate,
    customer: String,
    lines: Vec<LineItem>,
    #[serde(default)]
    discount: Option<Discount>,
    regime: R,
}

impl<R: TaxRegime<Error = TaxError> + Clone> TryFrom<RawInvoice<R>> for Invoice<R> {
    type Error = DocumentError;

    fn try_from(raw: RawInvoice<R>) -> Result<Self, Self::Error> {
        let invoice = Self::new(raw.number, raw.date, raw.customer, raw.lines, raw.regime)?;
        match raw.discount {
            Some(discount) => invoice.with_discount(discount),
            None => Ok(invoice),
        }
    }
}

impl<R: TaxRegime<Error = TaxError> + Clone> Invoice<R> {
    /// Create an invoice of `lines` to `customer`, taxed under `regime`
    ///
    /// The lines were validated when they were built.
    ///
    /// # Errors
    ///
    /// [`DocumentError::EmptyNumber`] for a blank number, or [`DocumentError::NoLines`].
    pub fn new(
        number: impl Into<String>,
        date: NaiveDate,
        customer: impl Into<String>,
        lines: Vec<LineItem>,
        regime: R,
    ) -> Result<Self, DocumentError> {
        let number = check_document(number.into(), &lines)?;
        Ok(Self {
            number,
            date,
            customer: customer.into(),
            lines,
            discount: None,
            regime,
        })
    }

    /// The invoice with `discount` taken off its subtotal, before tax
    ///
    /// # Errors
    ///
    /// [`DocumentError::InvalidDiscount`] if the discount is out of range or more than the
    /// [`Document::subtotal`].
    pub fn with_discount(self, discount: Discount) -> Result<Self, DocumentError> {
        let invoice = Self {
            discount: Some(discount),
            ..self
        };
        invoice.check_discount()?;
        Ok(invoice)
    }

    /// Journal entries that record this invoice: the receivable debited with the total, sales
    /// credited with the taxable value, and each tax credited to its account
    ///
    /// Pass them to [`TransactionBuilder::entry`](crate::ledger::TransactionBuilder::entry).
    ///
    /// # Errors
    ///
    /// [`DocumentError::Posting`] if the total is zero or a tax has no account, or any error from
    /// [`Document::tax_breakdown`].
    pub fn to_entries(&self, accounts: &SalesAccounts) -> Result<Vec<Entry>, DocumentError> {
        Ok(sales_entries(
            &self.tax_breakdown()?,
            accounts,
            PostingLeg::side,
        )?)
    }
}

impl<R: TaxRegime<Error = TaxError> + Clone> Document for Invoice<R> {
    type Line = LineItem;
    type Regime = R;
    type Error = DocumentError;

    fn number(&self) -> &str {
        &self.number
    }

    fn date(&self) -> NaiveDate {
        self.date
    }

    fn lines(&self) -> &[LineItem] {
        &self.lines
    }

    fn document_discount(&self) -> Option<&Discount> {
        self.discount.as_ref()
    }

    fn regime(&self) -> R {
        self.regime.clone()
    }
}

/// `number` trimmed, if it is not blank and `lines` is not empty
pub(super) fn check_document(number: String, lines: &[LineItem]) -> Result<String, DocumentError> {
    let number = number.trim().to_string();
    if number.is_empty() {
        Err(DocumentError::EmptyNumber)
    } else if lines.is_empty() {
        Err(DocumentError::NoLines)
    } else {
        Ok(number)
    }
}
