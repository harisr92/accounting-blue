//! Reconciliation of internal ledger records against external statements
//!
//! Reconciliation answers three questions about a period: which transactions agree between the
//! ledger and the bank, which exist on only one side, and which are probably the same transaction
//! recorded slightly differently.
//!
//! # Shape of the API
//!
//! [`ReconciliationEngine`] is pure and synchronous - hand it both sides and it hands back a
//! [`ReconciliationReport`]. Loading and saving are separate concerns, behind
//! [`ReconciliationStorage`]; importing statement files is behind [`ExternalDataParser`]. This
//! crate ships no parser implementations, because statement layouts are specific to each bank and
//! gateway.
//!
//! # Direction conventions
//!
//! Both [`LedgerTransaction`] and [`ExternalTransaction`] hold a **positive** amount plus an
//! [`EntryType`](crate::types::EntryType), matching [`Entry`](crate::types::Entry). Crucially the
//! external side's direction must be expressed from the *ledger's* point of view: a deposit shows
//! as a credit on a bank statement but is a debit in the ledger's bank account. Flipping that sign
//! is the importer's job, and getting it wrong will make everything look unmatched.
//!
//! # How matching works
//!
//! 1. **Reference numbers.** A shared UTR, RRN or cheque number backed by an equal amount and
//!    direction is conclusive, whatever the dates say. Ambiguous references are skipped.
//! 2. **Exact agreement** on date, amount and direction, resolved through a hash index.
//! 3. **Scored pairing.** Everything left over is scored across amount, date, description,
//!    direction and reference, then assigned best-first so a weak pairing cannot claim a
//!    counterpart that a stronger one wanted.
//! 4. **Suggestions.** Whatever is still unmatched is reported with its closest available
//!    counterparts, so a person can finish the job.
//!
//! Thresholds live in [`ReconciliationConfig`]. The result is independent of the order of either
//! input vector.
//!
//! # Example
//!
//! ```
//! use accounting_core::reconciliation::{
//!     ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationConfig,
//!     ReconciliationEngine,
//! };
//! use accounting_core::EntryType;
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! let booked = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
//! let settled = NaiveDate::from_ymd_opt(2024, 11, 16).unwrap();
//! let source = ExternalSource::BankStatement {
//!     bank_name: "SBI".to_string(),
//!     account_number: "12345678901".to_string(),
//! };
//!
//! // Booked a day before the bank settled it, but both carry the same UTR.
//! let ledger = vec![LedgerTransaction::new(
//!     "txn-1".to_string(),
//!     booked,
//!     BigDecimal::from(1000),
//!     "Payment from Acme Ltd".to_string(),
//!     EntryType::Debit,
//!     "bank".to_string(),
//! )
//! .with_reference("UTR12345")];
//!
//! let external = vec![ExternalTransaction::new(
//!     "ext-1".to_string(),
//!     settled,
//!     BigDecimal::from(1000),
//!     "NEFT/ACME LTD/UTR12345".to_string(),
//!     EntryType::Debit,
//!     source.clone(),
//! )
//! .with_reference("UTR12345")];
//!
//! let engine = ReconciliationEngine::new(ReconciliationConfig::default());
//! let report = engine.reconcile(ledger, external, source);
//!
//! assert_eq!(report.matched_count, 1);
//! assert!(report.is_fully_reconciled());
//! ```

pub mod config;
pub mod engine;
pub mod similarity;
pub mod storage;
pub mod types;

pub use config::ReconciliationConfig;
pub use engine::ReconciliationEngine;
pub use storage::{ExternalDataParser, ReconciliationStorage};
pub use types::*;
