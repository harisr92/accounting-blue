//! # Accounting Core
//!
//! A comprehensive accounting library providing double-entry bookkeeping,
//! GST calculations, and financial reporting capabilities.
//!
//! ## Features
//!
//! - **Double-entry bookkeeping**: Complete transaction validation and balance tracking
//! - **Account management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
//! - **Paginated responses**: Efficient pagination for large datasets with comprehensive metadata
//! - **GST calculations**: Indian GST compliance with CGST/SGST/IGST support
//! - **Financial reporting**: Balance sheets, income statements, and trial balance generation
//! - **Reconciliation**: Bank statement and payment gateway reconciliation
//! - **Storage abstraction**: Database-agnostic design with trait-based storage
//!
//! ## Quick Start
//!
//! ```rust
//! use accounting_core::{Ledger, AccountType, PaginationOption, PaginationParams};
//! use accounting_core::utils::memory_storage::MemoryStorage;
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a ledger with in-memory storage (for production, use a database storage)
//! let storage = MemoryStorage::new();
//! let mut ledger = Ledger::new(storage);
//!
//! // Create a cash account
//! let cash_account = ledger.create_account(
//!     "cash".to_string(),
//!     "Cash Account".to_string(),
//!     AccountType::Asset,
//!     None,
//! ).await?;
//!
//! // List all accounts with pagination (default: page 1, 50 items per page)
//! let pagination = PaginationParams::new(1, 50)?;
//! let result = ledger.list_accounts(PaginationOption::Paginated(pagination)).await?;
//! let accounts = result.to_paginated_response();
//! println!("Total accounts: {}, Current page: {} of {}",
//!          accounts.total_count,
//!          accounts.page,
//!          accounts.total_pages);
//! # Ok(())
//! # }
//! ```
//!
//! ## Pagination Support
//!
//! The library provides comprehensive pagination support for both accounts and transactions:
//!
//! ### Basic Pagination
//!
//! ```rust
//! use accounting_core::{Ledger, PaginationOption, PaginationParams, AccountType};
//! use accounting_core::utils::memory_storage::MemoryStorage;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = MemoryStorage::new();
//! let ledger = Ledger::new(storage);
//!
//! // Get first page with 10 accounts per page
//! let pagination = PaginationParams::new(1, 10)?;
//! let result = ledger.list_accounts(PaginationOption::Paginated(pagination)).await?;
//! let result = result.to_paginated_response();
//!
//! println!("Page {} of {} (showing {} of {} total accounts)",
//!          result.page,
//!          result.total_pages,
//!          result.items.len(),
//!          result.total_count);
//!
//! // Check if there are more pages
//! if result.has_next {
//!     let next_page = PaginationParams::new(2, 10)?;
//!     let next_result = ledger.list_accounts(PaginationOption::Paginated(next_page)).await?;
//!     // Process next page...
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Filtered Pagination
//!
//! ```rust
//! use accounting_core::{Ledger, PaginationOption, PaginationParams, AccountType};
//! use accounting_core::utils::memory_storage::MemoryStorage;
//! use chrono::NaiveDate;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = MemoryStorage::new();
//! let ledger = Ledger::new(storage);
//!
//! // Get only Asset accounts with pagination
//! let pagination = PaginationParams::new(1, 20)?;
//! let assets = ledger.list_accounts_by_type(AccountType::Asset, PaginationOption::Paginated(pagination)).await?;
//!
//! // Get transactions for a specific date range with pagination
//! let start_date = NaiveDate::from_ymd_opt(2024, 1, 1);
//! let end_date = NaiveDate::from_ymd_opt(2024, 12, 31);
//! let transactions = ledger.get_transactions(start_date, end_date, PaginationOption::Paginated(pagination)).await?;
//!
//! // Get transactions for a specific account with pagination
//! let account_txns = ledger.get_account_transactions(
//!     "cash",
//!     start_date,
//!     end_date,
//!     PaginationOption::Paginated(pagination)
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Pagination Metadata
//!
//! All paginated responses include comprehensive metadata for building user interfaces:
//!
//! ```rust
//! # use accounting_core::{Ledger, PaginationOption, PaginationParams};
//! # use accounting_core::utils::memory_storage::MemoryStorage;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let storage = MemoryStorage::new();
//! # let ledger = Ledger::new(storage);
//! let pagination = PaginationParams::new(2, 10)?;
//! let response = ledger.list_accounts(PaginationOption::Paginated(pagination)).await?;
//! let result = response.to_paginated_response();
//!
//! // Access pagination metadata
//! println!("Current page: {}", result.page);           // 2
//! println!("Page size: {}", result.page_size);         // 10
//! println!("Total items: {}", result.total_count);     // e.g., 25
//! println!("Total pages: {}", result.total_pages);     // e.g., 3
//! println!("Has next page: {}", result.has_next);      // true
//! println!("Has previous page: {}", result.has_previous); // true
//!
//! // Use metadata to build navigation
//! if result.has_previous {
//!     println!("Previous page available");
//! }
//! if result.has_next {
//!     println!("Next page available");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Reconciliation
//!
//! [`reconciliation::ReconciliationEngine`] matches ledger records against bank statements and
//! payment gateway settlements, classifying every record as matched, partially matched or
//! unmatched and suggesting counterparts for the leftovers. It is pure and synchronous; see the
//! [`reconciliation`] module docs for the matching passes and the direction conventions.
//!
//! ```rust
//! use accounting_core::reconciliation::{
//!     ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationEngine,
//! };
//! use accounting_core::EntryType;
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
//! let source = ExternalSource::BankStatement {
//!     bank_name: "SBI".to_string(),
//!     account_number: "12345678901".to_string(),
//! };
//!
//! let ledger = vec![LedgerTransaction::new(
//!     "txn-1".to_string(),
//!     date,
//!     BigDecimal::from(1000),
//!     "Payment from Acme Ltd".to_string(),
//!     EntryType::Debit,
//!     "bank".to_string(),
//! )];
//! let external = vec![ExternalTransaction::new(
//!     "stmt-1".to_string(),
//!     date,
//!     BigDecimal::from(1000),
//!     "NEFT/ACME LTD/0012".to_string(),
//!     EntryType::Debit,
//!     source.clone(),
//! )];
//!
//! let report = ReconciliationEngine::default().reconcile(ledger, external, source);
//! assert_eq!(report.matched_count, 1);
//! println!("Balance difference: {}", report.summary.difference);
//! ```
//!
//! ## Examples
//!
//! Check out the comprehensive examples in the `examples/` directory:
//!
//! - `pagination_demo.rs` - Complete pagination functionality walkthrough
//! - `api_pagination_patterns.rs` - REST API and GraphQL integration patterns
//! - `web_integration.rs` - Web framework integration examples
//! - `gst_invoice.rs` - GSTIN validation and B2B GST invoices
//! - `reconciliation.rs` - Reconciling a ledger account against a bank statement

use bigdecimal::BigDecimal;
use std::sync::LazyLock;

pub mod invoice;
pub mod ledger;
pub mod reconciliation;
pub mod tax;
pub mod traits;
pub mod types;
pub mod utils;

pub static ZERO: LazyLock<BigDecimal> = LazyLock::new(|| BigDecimal::from(0));

// Re-export commonly used types
pub use invoice::{GstBreakdown, GstInvoice, GstLineItem, Gstin, InvoiceError};
pub use ledger::*;
pub use tax::gst::*;
pub use traits::*;
pub use types::*;

// Re-export transaction patterns for convenience
pub use ledger::transaction::patterns;
