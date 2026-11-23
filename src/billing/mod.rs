//! Invoicing that works in any country: invoices, credit notes, discounts and ledger posting
//!
//! Tax is pluggable: an invoice prices its lines and asks its tax regime for the tax on each.
//! The Indian GST regime, with GSTIN, HSN/SAC codes and GSTR-1, builds on this module behind the
//! `india` feature.

pub mod discount;

pub use discount::{apportion, Discount, DiscountError};

#[cfg(test)]
mod tests;
