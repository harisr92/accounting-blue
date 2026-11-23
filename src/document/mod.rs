//! What every trade document shares, in any country: lines, discounts, tax and posting
//!
//! A document prices its [`LineItem`]s with [`price_lines`] and asks its [`TaxRegime`] for the tax
//! on each: [`NoTax`], [`FlatRate`] for a VAT or sales tax, or a regime of your own. The result is
//! a [`TaxBreakdown`] of named [`TaxComponent`]s, which posting turns into balanced journal
//! entries. [`validate_document`] runs the rules every document shares.
//!
//! The pieces are traits, so every document type reuses them: anything that is a
//! [`DocumentLine`] can be priced, and anything that is a [`Document`] gets the subtotal,
//! discount and breakdown arithmetic. The sales invoice and credit note in
//! [`sales`](crate::sales) are built this way, as is the Indian GST invoice in
//! `gst::invoice` behind the `india` feature.

pub mod discount;
pub mod error;
pub mod line;
pub mod posting;
pub mod pricing;
pub mod tax;
pub mod validation;

pub use discount::{apportion, Discount, DiscountError};
pub use error::DocumentError;
pub use line::{check_line, check_line_fields, DocumentLine, LineError, LineItem};
pub use posting::{legs_to_entries, PostingError, PostingLeg};
pub use pricing::{line_values, price_lines, Document, PricedLines};
pub use tax::{
    check_rate, FlatRate, NoTax, TaxBreakdown, TaxComponent, TaxError, TaxRegime, MAX_TAX_RATE,
};
pub use validation::{validate_document, Issue, Severity, ValidationIssue, ValidationReport};

#[cfg(test)]
mod tests;
