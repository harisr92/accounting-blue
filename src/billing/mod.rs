//! Invoicing that works in any country: invoices, credit notes, discounts and ledger posting
//!
//! Tax is pluggable: an invoice prices its lines and asks its tax regime for the tax on each.
//! The Indian GST regime, with GSTIN, HSN/SAC codes and GSTR-1, builds on this module behind the
//! `india` feature.

pub mod discount;
pub mod document;
pub mod error;
pub mod invoice;
pub mod note;
pub mod posting;
pub mod tax;
pub mod validation;

pub use discount::{apportion, Discount, DiscountError};
pub use document::{
    check_line, check_line_fields, line_values, price_lines, BillableLine, LineError, LineItem,
    PricedLines, TaxDocument,
};
pub use error::BillingError;
pub use invoice::Invoice;
pub use note::{CreditNote, OriginalDocument};
pub use posting::{breakdown_entries, posting_legs, PostingAccounts, PostingError, PostingLeg};
pub use tax::{
    check_rate, FlatRate, NoTax, TaxBreakdown, TaxComponent, TaxError, TaxRegime, MAX_TAX_RATE,
};
pub use validation::{validate_document, Issue, Severity, ValidationIssue, ValidationReport};

#[cfg(test)]
mod tests;
