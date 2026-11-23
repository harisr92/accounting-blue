//! Invoicing that works in any country: invoices, credit notes, discounts and ledger posting
//!
//! Tax is pluggable. An [`Invoice`] prices its [`LineItem`]s with [`price_lines`] and asks its
//! [`TaxRegime`] for the tax on each: [`NoTax`], [`FlatRate`] for a VAT or sales tax, or a regime
//! of your own. The result is a [`TaxBreakdown`] of named [`TaxComponent`]s, which
//! [`Invoice::to_entries`] posts to the accounts in [`PostingAccounts`]. A [`CreditNote`] against
//! an invoice posts the reverse. [`validate_document`] runs the rules every document shares.
//!
//! The pieces are traits, so other document types can reuse them: anything that is a
//! [`BillableLine`] can be priced, and anything that is a [`TaxDocument`] gets the subtotal,
//! discount and breakdown arithmetic. The Indian GST invoice, with GSTIN, HSN/SAC codes,
//! CGST/SGST/IGST and GSTR-1, is built this way in the `invoice` module behind the `india`
//! feature.

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
