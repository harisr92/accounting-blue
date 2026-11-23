//! Credit notes against sales invoices under any tax regime
//!
//! A seller issues a [`CreditNote`] to reduce the value of an invoice it issued: goods returned,
//! a discount after the sale, or an amount over-billed. The note is built from the [`Invoice`] it
//! corrects, so it bills the same customer under the same regime, and it records the invoice as
//! an [`OriginalInvoice`]. It may not be dated before the invoice, nor credit more than the
//! invoice was worth. [`CreditNote::to_entries`] posts it as the reverse of an invoice.

use crate::document::error::DocumentError;
use crate::document::line::LineItem;
use crate::document::posting::{breakdown_entries, PostingAccounts};
use crate::document::pricing::Document;
use crate::document::tax::{TaxError, TaxRegime};
use crate::sales::invoice::{check_document, Invoice};
use crate::types::Entry;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// The invoice a credit note corrects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalInvoice {
    /// Invoice number
    pub number: String,
    /// Date of issue
    pub date: NaiveDate,
    /// Invoice value, tax included: the most the note may credit
    pub value: BigDecimal,
}

/// A credit note: the seller reduces the value of an invoice it issued
///
/// # Example
///
/// ```
/// use accounting_core::document::{FlatRate, LineItem, PostingAccounts};
/// use accounting_core::sales::{CreditNote, Invoice};
/// use accounting_core::EntryType;
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// # fn main() -> Result<(), accounting_core::Error> {
/// let line = |quantity| LineItem::new("Chair", BigDecimal::from(quantity), BigDecimal::from(50),
///     BigDecimal::from(20));
/// let date = |day| NaiveDate::from_ymd_opt(2025, 3, day).unwrap();
/// let invoice = Invoice::new("INV-001", date(1), "Acme Ltd", vec![line(10)?], FlatRate::new("VAT"))?;
///
/// // Two chairs come back
/// let note = CreditNote::new(&invoice, "CN-001", date(5), vec![line(2)?])?;
/// let accounts = PostingAccounts::new("receivable", "sales returns").with_tax("VAT", "vat");
/// let entries = note.to_entries(&accounts)?;
/// assert_eq!(entries[0].entry_type, EntryType::Credit);
/// assert_eq!(entries[0].amount, BigDecimal::from(120));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditNote<R> {
    /// Note number
    pub number: String,
    /// Date of issue
    pub date: NaiveDate,
    /// The invoice the note corrects
    pub original: OriginalInvoice,
    /// Who is credited, the same as on the invoice
    pub customer: String,
    /// What is credited: the value and tax each line takes off the invoice
    pub lines: Vec<LineItem>,
    /// The regime the invoice was taxed under
    pub regime: R,
}

impl<R: TaxRegime<Error = TaxError> + Clone> CreditNote<R> {
    /// Create a credit note of `lines` against `invoice`
    ///
    /// # Errors
    ///
    /// [`DocumentError::EmptyNumber`] or [`DocumentError::NoLines`],
    /// [`DocumentError::NoteBeforeOriginal`] for a note dated before the invoice,
    /// [`DocumentError::CreditExceedsInvoice`] for a note worth more than the invoice, or any
    /// error from [`Document::tax_breakdown`].
    pub fn new(
        invoice: &Invoice<R>,
        number: impl Into<String>,
        date: NaiveDate,
        lines: Vec<LineItem>,
    ) -> Result<Self, DocumentError> {
        let number = check_document(number.into(), &lines)?;
        if date < invoice.date {
            return Err(DocumentError::NoteBeforeOriginal {
                note_date: date,
                original_date: invoice.date,
            });
        }
        let note = Self {
            number,
            date,
            original: OriginalInvoice {
                number: invoice.number.clone(),
                date: invoice.date,
                value: invoice.tax_breakdown()?.total,
            },
            customer: invoice.customer.clone(),
            lines,
            regime: invoice.regime.clone(),
        };
        let credited = note.tax_breakdown()?.total;
        if credited > note.original.value {
            return Err(DocumentError::CreditExceedsInvoice {
                credited,
                invoice_value: note.original.value,
            });
        }
        Ok(note)
    }

    /// Journal entries that record this note: an invoice's legs on the opposite sides
    ///
    /// Pass a sales returns account as `accounts.sales` to keep returns apart from sales.
    ///
    /// # Errors
    ///
    /// As [`Invoice::to_entries`].
    pub fn to_entries(&self, accounts: &PostingAccounts) -> Result<Vec<Entry>, DocumentError> {
        Ok(breakdown_entries(
            &self.tax_breakdown()?,
            accounts,
            |leg| leg.side().opposite(),
        )?)
    }
}

impl<R: TaxRegime<Error = TaxError> + Clone> Document for CreditNote<R> {
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

    fn regime(&self) -> R {
        self.regime.clone()
    }
}
