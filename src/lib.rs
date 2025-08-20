//! # Accounting Core
//!
//! A comprehensive accounting library providing double-entry bookkeeping,
//! GST calculations, and financial reporting capabilities.
//!
//! ## Features
//!
//! - **Double-entry bookkeeping**: Complete transaction validation and balance tracking
//! - **Account management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
//! - **GST calculations**: Indian GST compliance with CGST/SGST/IGST support
//! - **Financial reporting**: Balance sheets, income statements, and trial balance generation
//! - **Reconciliation**: Bank statement and payment gateway reconciliation
//! - **Storage abstraction**: Database-agnostic design with trait-based storage
//!
//! ## Quick Start
//!
//! ```rust
//! use accounting_core::{Ledger, Account, AccountType, Transaction, Entry, EntryType};
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! // This example shows basic usage - you need to implement LedgerStorage trait
//! // let storage = YourStorageImplementation::new();
//! // let mut ledger = Ledger::new(storage);
//! ```

pub mod ledger;
pub mod reconciliation;
pub mod tax;
pub mod traits;
pub mod types;
pub mod utils;

// Re-export commonly used types
pub use ledger::*;
pub use tax::gst::*;
pub use traits::*;
pub use types::*;

// Re-export transaction patterns for convenience
pub use ledger::transaction::patterns;
