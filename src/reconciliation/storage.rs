//! Extension points for persistence and external data import
//!
//! Neither trait is required to run [`ReconciliationEngine`](crate::reconciliation::ReconciliationEngine),
//! which is pure. They exist so an application can plug its own database and statement formats in
//! without this crate taking on a database or HTTP dependency.

use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::reconciliation::types::*;
use crate::types::{ListResponse, PaginationOption};

/// Persistence for reconciliation runs
///
/// Implement this over PostgreSQL, SQLite or anything else; see
/// [`MemoryReconciliationStorage`](crate::utils::memory_storage::MemoryReconciliationStorage) for a
/// reference implementation.
#[async_trait]
pub trait ReconciliationStorage: Send + Sync {
    /// Load one account's legs of the transactions booked in a date range, inclusive
    async fn get_ledger_transactions(
        &self,
        account_id: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> ReconciliationResult<Vec<LedgerTransaction>>;

    /// Store a completed report
    async fn save_reconciliation_report(
        &mut self,
        report: &ReconciliationReport,
    ) -> ReconciliationResult<()>;

    /// Fetch a report by id
    async fn get_reconciliation_report(
        &self,
        report_id: Uuid,
    ) -> ReconciliationResult<Option<ReconciliationReport>>;

    /// List the reports covering an account, newest first
    async fn list_reconciliation_reports(
        &self,
        account_id: &str,
        pagination: PaginationOption,
    ) -> ReconciliationResult<ListResponse<ReconciliationReport>>;

    /// Record that a ledger transaction has been reconciled against an external one
    async fn mark_transaction_as_reconciled(
        &mut self,
        ledger_id: &str,
        external_id: &str,
        reconciliation_id: Uuid,
    ) -> ReconciliationResult<()>;
}

/// Turns raw statement data into [`ExternalTransaction`]s
///
/// Statement layouts vary by bank and by gateway, so this crate ships no implementations. An
/// implementation is responsible for one thing beyond parsing: expressing `entry_type` from the
/// **ledger's** point of view. Money arriving in a bank account is a credit on the bank's
/// statement but a debit in the ledger's bank account, and the engine compares them directly.
#[async_trait]
pub trait ExternalDataParser: Send + Sync {
    /// Parse a statement, settlement file or API payload
    async fn parse(&self, data: &[u8]) -> ReconciliationResult<Vec<ExternalTransaction>>;
}
