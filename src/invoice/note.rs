//! Credit notes against GST invoices
//!
//! A seller issues a [`CreditNote`] (Section 34 of the CGST Act) to reduce the value or tax of an
//! invoice it issued: goods returned, a discount after the sale, or an amount over-billed. The note
//! is built from the [`GstInvoice`] it corrects, so it has the same seller, buyer and place of
//! supply, and it records the invoice as an [`OriginalInvoice`]: its number, date and
//! [`SupplyKind`], which decides where GSTR-1 reports the note.
//!
//! Its lines are what is credited, priced like invoice lines; [`GstDocument`] gives it the same
//! tax arithmetic as an invoice.

use super::types::{
    supply_kind_for, validate_invoice_number, GstDocument, GstInvoice, GstLineItem, Gstin,
    InvoiceError, Recipient, SupplyKind,
};
use bigdecimal::BigDecimal;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// First month of the Indian financial year (April)
const FINANCIAL_YEAR_START_MONTH: u32 = 4;

/// Month and day, in the calendar year a financial year ends, after which a credit note can no
/// longer be issued against that year's invoices (Section 34(2), as amended in 2022)
const CREDIT_NOTE_DEADLINE: (u32, u32) = (11, 30);

/// The invoice a credit note corrects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalInvoice {
    /// Invoice number
    pub number: String,
    /// Date of issue
    pub date: NaiveDate,
    /// How GSTR-1 reported the invoice, which decides where it reports the note
    pub kind: SupplyKind,
    /// Invoice value, tax included: the most the note may credit
    pub value: BigDecimal,
}

/// A credit note: the seller reduces the value or tax of an invoice it issued
///
/// Deserialising goes through the same checks as [`CreditNote::new`], except that the original
/// invoice is given by its [`OriginalInvoice`] record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCreditNote")]
pub struct CreditNote {
    /// Note number: at most 16 characters of letters, digits, `-` and `/` (Rule 53)
    pub note_number: String,
    /// Date of issue
    pub note_date: NaiveDate,
    /// The invoice the note corrects
    pub original: OriginalInvoice,
    /// Supplier's GSTIN, the same as on the invoice
    pub seller_gstin: Gstin,
    /// Recipient, the same as on the invoice
    pub buyer: Recipient,
    /// What is credited: the value and tax each line takes off the invoice
    pub line_items: Vec<GstLineItem>,
}

/// Unvalidated shape of a [`CreditNote`], used when deserialising
#[derive(Deserialize)]
struct RawCreditNote {
    note_number: String,
    note_date: NaiveDate,
    original: OriginalInvoice,
    seller_gstin: Gstin,
    buyer: Recipient,
    line_items: Vec<GstLineItem>,
}

impl TryFrom<RawCreditNote> for CreditNote {
    type Error = InvoiceError;

    fn try_from(raw: RawCreditNote) -> Result<Self, Self::Error> {
        Self {
            note_number: raw.note_number,
            note_date: raw.note_date,
            original: raw.original,
            seller_gstin: raw.seller_gstin,
            buyer: raw.buyer,
            line_items: raw.line_items,
        }
        .checked()
    }
}

impl CreditNote {
    /// Create a validated credit note against `invoice`
    ///
    /// The note takes the invoice's seller and buyer, and records its number, date and supply
    /// kind.
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidInvoiceNumber`] for a note number that breaks Rule 53,
    /// [`InvoiceError::EmptyInvoice`] for no lines, [`InvoiceError::NoteBeforeOriginal`] for a
    /// note dated before the invoice, [`InvoiceError::CreditExceedsInvoice`] for a note whose
    /// total is more than the invoice's, or any error from [`GstInvoice::breakdown`].
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::invoice::{CreditNote, GstDocument, GstInvoice, GstLineItem, Gstin};
    /// use bigdecimal::BigDecimal;
    /// use chrono::NaiveDate;
    ///
    /// # fn main() -> Result<(), accounting_core::Error> {
    /// let item = |quantity| GstLineItem::new("998314", "IT consulting", BigDecimal::from(quantity),
    ///     BigDecimal::from(1000), BigDecimal::from(18));
    /// let invoice = GstInvoice::new("INV-001", NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
    ///     Gstin::parse("27AAPFU0939F1ZV")?, Gstin::parse("27AAPFU0939F2ZU")?, vec![item(10)?])?;
    ///
    /// // Two of the ten units are credited back
    /// let note = CreditNote::new(&invoice, "CN-001", NaiveDate::from_ymd_opt(2024, 11, 20).unwrap(),
    ///     vec![item(2)?])?;
    /// assert_eq!(note.original.number, "INV-001");
    /// assert_eq!(note.breakdown()?.total, BigDecimal::from(2360));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        invoice: &GstInvoice,
        note_number: impl Into<String>,
        note_date: NaiveDate,
        line_items: Vec<GstLineItem>,
    ) -> Result<Self, InvoiceError> {
        let value = invoice.breakdown()?.total;
        Self {
            note_number: note_number.into(),
            note_date,
            original: OriginalInvoice {
                number: invoice.invoice_number.clone(),
                date: invoice.invoice_date,
                kind: supply_kind_for(invoice, &value),
                value,
            },
            seller_gstin: invoice.seller_gstin.clone(),
            buyer: invoice.buyer.clone(),
            line_items,
        }
        .checked()
    }

    /// The note, if it meets the construction rules
    fn checked(self) -> Result<Self, InvoiceError> {
        validate_invoice_number(&self.note_number)?;
        validate_invoice_number(&self.original.number)?;
        if self.line_items.is_empty() {
            return Err(InvoiceError::EmptyInvoice);
        }
        if self.predates_original() {
            return Err(InvoiceError::NoteBeforeOriginal {
                note_date: self.note_date,
                original_date: self.original.date,
            });
        }
        if !self.original_kind_fits_buyer() {
            return Err(InvoiceError::OriginalKindMismatch {
                kind: self.original.kind,
                buyer: self.buyer,
            });
        }
        if let Some(credited) = self.credit_over_value()? {
            return Err(InvoiceError::CreditExceedsInvoice {
                credited,
                invoice_value: self.original.value,
            });
        }
        Ok(self)
    }

    /// Whether the note is dated before the invoice it corrects
    pub(super) fn predates_original(&self) -> bool {
        self.note_date < self.original.date
    }

    /// The note's total, tax included, when it is more than its invoice's value, the most it may
    /// credit
    ///
    /// # Errors
    ///
    /// Any error from [`GstDocument::breakdown`].
    pub(super) fn credit_over_value(&self) -> Result<Option<BigDecimal>, InvoiceError> {
        let credited = self.breakdown()?.total;
        Ok((credited > self.original.value).then_some(credited))
    }

    /// Whether the original's supply kind is one an invoice to this buyer could have: B2B for a
    /// registered buyer, B2CS for an unregistered one, or B2CL for an unregistered one in another
    /// state
    pub(super) fn original_kind_fits_buyer(&self) -> bool {
        match self.original.kind {
            SupplyKind::B2b => self.buyer.is_registered(),
            SupplyKind::B2cl => !self.buyer.is_registered() && self.is_inter_state(),
            SupplyKind::B2cs => !self.buyer.is_registered(),
        }
    }

    /// Last day this note may be issued: [`credit_note_deadline`] for the original's date
    #[must_use]
    pub fn deadline(&self) -> NaiveDate {
        credit_note_deadline(self.original.date)
    }
}

impl GstDocument for CreditNote {
    fn number(&self) -> &str {
        &self.note_number
    }

    fn date(&self) -> NaiveDate {
        self.note_date
    }

    fn supply_date(&self) -> NaiveDate {
        self.original.date
    }

    fn seller_gstin(&self) -> &Gstin {
        &self.seller_gstin
    }

    fn buyer(&self) -> &Recipient {
        &self.buyer
    }

    fn line_items(&self) -> &[GstLineItem] {
        &self.line_items
    }
}

/// Last day a credit note may be issued against an invoice dated `invoice_date`: 30 November after
/// the end of the invoice's financial year (Section 34(2))
///
/// The Act's deadline is that day or the day the annual return for the year is filed, whichever
/// is earlier. The annual return's date is not known here, so a seller who files it before 30
/// November must apply the earlier date itself.
///
/// An invoice of 15 November 2024 falls in the financial year April 2024 to March 2025, so its
/// credit notes must be issued by 30 November 2025.
#[must_use]
pub fn credit_note_deadline(invoice_date: NaiveDate) -> NaiveDate {
    let year_ends_in = if invoice_date.month() >= FINANCIAL_YEAR_START_MONTH {
        invoice_date.year() + 1
    } else {
        invoice_date.year()
    };
    let (month, day) = CREDIT_NOTE_DEADLINE;
    NaiveDate::from_ymd_opt(year_ends_in, month, day).unwrap_or(NaiveDate::MAX)
}
