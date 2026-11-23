//! The sales side: invoices issued to customers and credit notes against them
//!
//! An [`Invoice`] bills a customer for [`LineItem`](crate::document::LineItem)s under a
//! [`TaxRegime`](crate::document::TaxRegime); a [`CreditNote`] reduces an invoice, for goods
//! returned, a discount after the sale or an amount over-billed. Both post to the ledger: an
//! invoice debits the receivable and credits revenue and output tax, and a credit note posts the
//! reverse.

pub mod credit_note;
pub mod invoice;

pub use credit_note::{CreditNote, OriginalDocument};
pub use invoice::Invoice;

#[cfg(test)]
mod tests;
