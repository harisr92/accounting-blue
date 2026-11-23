//! Why an invoice or credit note could not be built, priced or posted

use crate::document::discount::DiscountError;
use crate::document::posting::PostingError;
use crate::document::tax::TaxError;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

/// Why an invoice or credit note could not be built, priced or posted
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    /// The document number is empty or only whitespace
    #[error("a document number cannot be empty")]
    EmptyNumber,
    /// The document has no lines
    #[error("a document needs at least one line")]
    NoLines,
    /// The document discount is out of range or more than the subtotal
    #[error("invalid document discount: {0}")]
    InvalidDiscount(#[from] DiscountError),
    /// A credit note is dated before the invoice it corrects
    #[error("credit note dated {note_date} is before its invoice dated {original_date}")]
    NoteBeforeOriginal {
        /// Date of the note
        note_date: NaiveDate,
        /// Date of the invoice it corrects
        original_date: NaiveDate,
    },
    /// A credit note credits more than the invoice it corrects was worth
    #[error("credit note total {credited} is more than its invoice's value {invoice_value}")]
    CreditExceedsInvoice {
        /// The note's total, tax included
        credited: BigDecimal,
        /// The invoice's value, tax included
        invoice_value: BigDecimal,
    },
    /// The regime could not tax a line
    #[error(transparent)]
    Tax(#[from] TaxError),
    /// The document could not be posted to the accounts
    #[error(transparent)]
    Posting(#[from] PostingError),
}
