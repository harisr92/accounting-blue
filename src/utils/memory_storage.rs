//! In-memory storage implementations for tests, examples and prototyping
//!
//! Both stores are plain maps owned by their holder: writes take `&mut self`, so there are no
//! locks to poison. Share one between tasks by wrapping the owner (for example the
//! [`Ledger`](crate::Ledger)) in your own `Mutex`.

use async_trait::async_trait;
use chrono::NaiveDate;
use std::cmp::Ordering;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{LedgerError, LedgerResult};
use crate::reconciliation::{
    LedgerTransaction, ReconciliationReport, ReconciliationResult, ReconciliationStorage,
};
use crate::traits::{AccountStore, TransactionStore};
use crate::types::{
    Account, AccountType, ListResponse, PaginationOption, Transaction, TransactionFilter,
};

/// In-memory ledger storage
#[derive(Debug, Clone, Default)]
pub struct MemoryStorage {
    accounts: HashMap<String, Account>,
    transactions: HashMap<String, Transaction>,
}

impl MemoryStorage {
    /// Create an empty store
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.accounts.clear();
        self.transactions.clear();
    }
}

/// Newest first, then by id so the order is stable
fn newest_first(a: &Transaction, b: &Transaction) -> Ordering {
    b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id))
}

/// Clone the values that pass `keep`, sorted by `order`
fn select<'a, T: Clone + 'a>(
    values: impl Iterator<Item = &'a T>,
    keep: impl Fn(&T) -> bool,
    order: impl Fn(&T, &T) -> Ordering,
) -> Vec<T> {
    let mut selected: Vec<T> = values.filter(|value| keep(value)).cloned().collect();
    selected.sort_by(order);
    selected
}

#[async_trait]
impl AccountStore for MemoryStorage {
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()> {
        self.accounts.insert(account.id.clone(), account.clone());
        Ok(())
    }

    async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>> {
        Ok(self.accounts.get(account_id).cloned())
    }

    async fn list_accounts(
        &self,
        account_type: Option<AccountType>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        let accounts = select(
            self.accounts.values(),
            |account| account_type.is_none_or(|t| account.account_type == t),
            |a, b| a.id.cmp(&b.id),
        );
        Ok(pagination.paginate(accounts))
    }

    async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        let stored = self
            .accounts
            .get_mut(&account.id)
            .ok_or_else(|| LedgerError::AccountNotFound(account.id.clone()))?;
        stored.clone_from(account);
        Ok(())
    }

    async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()> {
        self.accounts
            .remove(account_id)
            .map(drop)
            .ok_or_else(|| LedgerError::AccountNotFound(account_id.to_string()))
    }
}

#[async_trait]
impl TransactionStore for MemoryStorage {
    async fn save_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        self.transactions
            .insert(transaction.id.clone(), transaction.clone());
        Ok(())
    }

    async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>> {
        Ok(self.transactions.get(transaction_id).cloned())
    }

    async fn list_transactions(
        &self,
        filter: &TransactionFilter,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        let transactions = select(
            self.transactions.values(),
            |transaction| filter.matches(transaction),
            newest_first,
        );
        Ok(pagination.paginate(transactions))
    }

    async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        let stored = self
            .transactions
            .get_mut(&transaction.id)
            .ok_or_else(|| LedgerError::TransactionNotFound(transaction.id.clone()))?;
        stored.clone_from(transaction);
        Ok(())
    }

    async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()> {
        self.transactions
            .remove(transaction_id)
            .map(drop)
            .ok_or_else(|| LedgerError::TransactionNotFound(transaction_id.to_string()))
    }
}

/// In-memory reconciliation storage
///
/// A reference implementation of [`ReconciliationStorage`]. Ledger transactions are seeded with
/// [`add_ledger_transaction`](Self::add_ledger_transaction) rather than derived from a ledger, so
/// the reconciliation side can be exercised in isolation.
#[derive(Debug, Clone, Default)]
pub struct MemoryReconciliationStorage {
    ledger_transactions: Vec<LedgerTransaction>,
    reports: HashMap<Uuid, ReconciliationReport>,
    reconciled: HashMap<String, (String, Uuid)>,
}

impl MemoryReconciliationStorage {
    /// Create an empty store
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a ledger transaction leg
    pub fn add_ledger_transaction(&mut self, transaction: LedgerTransaction) {
        self.ledger_transactions.push(transaction);
    }

    /// Every reconciliation mark recorded so far, keyed by ledger transaction id
    #[must_use]
    pub fn reconciled_marks(&self) -> &HashMap<String, (String, Uuid)> {
        &self.reconciled
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.ledger_transactions.clear();
        self.reports.clear();
        self.reconciled.clear();
    }
}

#[async_trait]
impl ReconciliationStorage for MemoryReconciliationStorage {
    async fn get_ledger_transactions(
        &self,
        account_id: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> ReconciliationResult<Vec<LedgerTransaction>> {
        Ok(select(
            self.ledger_transactions.iter(),
            |transaction| {
                transaction.account_id == account_id
                    && (start_date..=end_date).contains(&transaction.date)
            },
            |a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)),
        ))
    }

    async fn save_reconciliation_report(
        &mut self,
        report: &ReconciliationReport,
    ) -> ReconciliationResult<()> {
        self.reports.insert(report.id, report.clone());
        Ok(())
    }

    async fn get_reconciliation_report(
        &self,
        report_id: Uuid,
    ) -> ReconciliationResult<Option<ReconciliationReport>> {
        Ok(self.reports.get(&report_id).cloned())
    }

    async fn list_reconciliation_reports(
        &self,
        account_id: &str,
        pagination: PaginationOption,
    ) -> ReconciliationResult<ListResponse<ReconciliationReport>> {
        let reports = select(
            self.reports.values(),
            |report| report.account_ids.iter().any(|id| id == account_id),
            // Newest first, with the id as a stable tie-breaker
            |a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            },
        );
        Ok(pagination.paginate(reports))
    }

    async fn mark_transaction_as_reconciled(
        &mut self,
        ledger_id: &str,
        external_id: &str,
        reconciliation_id: Uuid,
    ) -> ReconciliationResult<()> {
        self.reconciled.insert(
            ledger_id.to_string(),
            (external_id.to_string(), reconciliation_id),
        );
        Ok(())
    }
}
