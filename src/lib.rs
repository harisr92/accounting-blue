//! # Accounting Core
//!
//! A country-neutral accounting library providing double-entry bookkeeping, financial
//! reporting and reconciliation. Indian GST support is an opt-in Cargo feature.
//!
//! ## Features
//!
//! - **Double-entry bookkeeping**: Complete transaction validation and balance tracking
//! - **Invoicing**: Invoices, credit notes and discounts under a pluggable tax regime (none, a flat
//!   VAT or sales tax, or your own), posted to the ledger
//! - **Account management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
//! - **Paginated responses**: Efficient pagination for large datasets with comprehensive metadata
//! - **Financial reporting**: Balance sheets, income statements, and trial balance generation
//! - **Reconciliation**: Bank statement and payment gateway reconciliation
//! - **Storage abstraction**: Database-agnostic design with trait-based storage
//!
//! ## Optional features
//!
//! | Feature | Adds |
//! |---------|------|
//! | `india` | Indian GST on top of the invoicing core: rates and CGST/SGST/IGST maths (`tax`), GSTIN, HSN/SAC master data, GST invoices and credit notes (`invoice`), GSTR-1 returns (`returns`), and Indian amount formatting (`utils::formatting`) |
//! | `pdf` | Renders GST invoices to PDF; turns on `india` |
//!
//! ```toml
//! [dependencies]
//! accounting-core = { version = "0.4", features = ["india"] }
//! ```
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
//! let accounts = result.into_paginated_response();
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
//! let result = result.into_paginated_response();
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
//! let transactions = ledger.list_transactions(start_date, end_date, PaginationOption::Paginated(pagination)).await?;
//!
//! // Get transactions for a specific account with pagination
//! let account_txns = ledger.list_account_transactions(
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
//! let result = response.into_paginated_response();
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
//! ## Invoicing
//!
//! [`document`] builds invoices and credit notes, works out their tax under a [`TaxRegime`] and
//! posts them as balanced journal entries.
//!
//! ```rust
//! use accounting_core::{FlatRate, Invoice, LineItem, PostingAccounts, TaxDocument};
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # fn main() -> Result<(), accounting_core::Error> {
//! let line = LineItem::new("Consulting", BigDecimal::from(10), BigDecimal::from(100),
//!     BigDecimal::from(20))?;
//! let invoice = Invoice::new("INV-001", NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
//!     "Acme Ltd", vec![line], FlatRate::new("VAT"))?;
//! assert_eq!(invoice.tax_breakdown()?.total, BigDecimal::from(1200));
//!
//! let accounts = PostingAccounts::new("1200", "4000").with_tax("VAT", "2200");
//! let entries = invoice.to_entries(&accounts)?; // Dr 1200 1200, Cr 4000 1000, Cr 2200 200
//! assert_eq!(entries.len(), 3);
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
//! let report = ReconciliationEngine::default().reconcile(&ledger, &external, source);
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
//! - `reconciliation.rs` - Reconciling a ledger account against a bank statement
//! - `invoice.rs` - A VAT invoice and a credit note, posted to the ledger
//! - `gst_calculations.rs` - CGST/SGST/IGST maths (needs the `india` feature)
//! - `gst_invoice.rs` - GSTIN validation, HSN/SAC rate lookup and B2B GST invoices (needs `india`)
//! - `gstr1_export.rs` - Aggregating a month of invoices into GSTR-1 and exporting it as JSON
//!   (needs `india`)
//! - `gst_invoice_pdf.rs` - Rendering a GST invoice to PDF (needs the `pdf` feature)

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![cfg_attr(not(test), warn(clippy::too_many_lines))]

pub mod document;
pub mod error;
#[cfg(feature = "india")]
pub mod gst;
pub mod ledger;
pub mod money;
pub mod reconciliation;
pub mod reports;
pub mod traits;
pub mod types;
pub mod utils;

pub use document::{
    apportion, BillableLine, BillingError, CreditNote, Discount, DiscountError, FlatRate, Invoice,
    LineItem, NoTax, PostingAccounts, PostingLeg, TaxBreakdown, TaxComponent, TaxDocument,
    TaxRegime,
};
pub use error::{BoxError, Error, FieldError, LedgerError, LedgerResult, PaginationError, Result};
#[cfg(feature = "india")]
pub use gst::invoice::{state_label, state_name, InvoiceDetails, PaymentDetails};
#[cfg(feature = "india")]
pub use gst::invoice::{
    validate_credit_note, validate_invoice, ComplianceIssue, GstBreakdown, GstCreditNote,
    GstDocument, GstInvoice, GstLineItem, Gstin, HsnMaster, HsnSacEntry, HsnSacKind,
    InvoiceAccounts, InvoiceError, InvoiceParties, InvoiceParty, InvoicePrint,
    InvoiceValidationReport, PostingError, Recipient, Severity, StateCode, SupplyKind,
    SupplyTreatment,
};
#[cfg(feature = "pdf")]
pub use gst::invoice::{PdfError, PdfFont, PdfOptions};
#[cfg(feature = "india")]
pub use gst::returns::{Gstr1Error, Gstr1Return, ReturnPeriod};
#[cfg(feature = "india")]
pub use gst::tax::{GstCalculation, GstCalculator, GstCategory, GstError, GstRate};
pub use ledger::{
    patterns, BillPaymentWithTaxParams, InvoiceWithTaxParams, Ledger, TransactionBuilder,
    STANDARD_CHART,
};
pub use reports::{
    BalanceSheet, CashFlowItem, CashFlowStatement, IncomeStatement, LedgerIntegrityReport,
};
pub use traits::{
    AccountStore, AccountValidator, LedgerStorage, TransactionStore, TransactionValidator,
};
pub use types::{
    Account, AccountBalance, AccountType, Entry, EntryType, ListResponse, PaginatedResponse,
    PaginationOption, PaginationParams, Transaction, TransactionFilter, TrialBalance,
};
pub use utils::validation::{DefaultAccountValidator, DefaultTransactionValidator};

#[cfg(test)]
mod tests;
