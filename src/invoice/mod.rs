//! GST invoices under Indian rules
//!
//! [`GstInvoice`] models a B2B tax invoice: invoice number, date, seller and buyer [`Gstin`], and
//! [`GstLineItem`]s carrying an HSN/SAC code. Every identifier is validated on construction, so an
//! invoice that exists is well-formed. Whether tax is split as CGST + SGST or charged as IGST
//! follows from the seller and buyer state codes; [`GstInvoice::breakdown`] returns the totals as
//! a [`GstBreakdown`].
//!
//! [`HsnMaster`] holds common HSN/SAC codes with their default GST rates;
//! [`GstLineItem::with_default_rate`] builds a line at the rate for its code.
//!
//! [`validate_invoice`] checks an invoice against the compliance rules (future dates, missing or
//! unknown HSN/SAC codes, amounts, rates and parties) and returns an [`InvoiceValidationReport`]
//! of every [`ComplianceIssue`] found. [`GstInvoice::to_entries`] posts a compliant invoice to the
//! ledger through the accounts named in [`InvoiceAccounts`]; the posting balances by
//! construction, since the total is the taxable value plus the tax.
//!
//! [`InvoicePrint::from_invoice`] builds the printable view of an invoice from the
//! [`InvoiceParties`] that issue and receive it, with amounts in Indian digit grouping and the
//! total in words. With the `pdf` feature, `GstInvoice::to_pdf` renders that view as an A4 PDF
//! (see the `pdf` module).
//!
//! The tax arithmetic comes from [`crate::tax::gst`]. The invoice types are also re-exported at the
//! crate root. Validation failures are typed: [`InvoiceError`] carries a [`GstinError`],
//! [`InvoiceNumberError`] or [`LineItemError`] saying which rule was broken.
//!
//! # Example
//!
//! ```
//! use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin};
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # fn main() -> Result<(), accounting_core::invoice::InvoiceError> {
//! let item = GstLineItem::new(
//!     "998314".to_string(),
//!     "IT consulting".to_string(),
//!     BigDecimal::from(10),
//!     BigDecimal::from(1500),
//!     BigDecimal::from(18),
//! )?;
//!
//! let invoice = GstInvoice::new(
//!     "INV/2024-25/001".to_string(),
//!     NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
//!     Gstin::parse("27AAPFU0939F1ZV")?,
//!     Gstin::parse("27AAPFU0939F2ZU")?,
//!     vec![item],
//! )?;
//!
//! let breakdown = invoice.breakdown()?;
//! assert_eq!(breakdown.cgst, BigDecimal::from(1350));
//! assert_eq!(breakdown.sgst, BigDecimal::from(1350));
//! assert_eq!(breakdown.total, BigDecimal::from(17700));
//! # Ok(())
//! # }
//! ```

pub mod hsn_lookup;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod posting;
pub mod print;
pub mod types;
pub mod validation;

pub use hsn_lookup::{HsnMaster, HsnSacEntry, HsnSacKind};
#[cfg(feature = "pdf")]
pub use pdf::{
    render_pdf, FontFace, PdfError, PdfFont, PdfOptions, DEFAULT_CURRENCY_LABEL,
    DEFAULT_FOOTER_NOTE,
};
pub use posting::{posting_legs, InvoiceAccounts, PostingError, PostingLeg};
pub use print::{
    paginate_rows, InvoiceParties, InvoiceParty, InvoicePrint, PartyError, PartyRole, PrintRow,
    RowCapacity, TaxLine,
};
pub use types::{
    GstBreakdown, GstInvoice, GstLineItem, Gstin, GstinError, InvoiceError, InvoiceNumberError,
    LineItemError,
};
pub use validation::{validate_invoice, ComplianceIssue, InvoiceValidationReport, Severity};

#[cfg(test)]
mod tests;
