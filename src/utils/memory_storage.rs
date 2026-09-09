//! In-memory storage implementation for testing

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

use crate::reconciliation::{
    LedgerTransaction, ReconciliationReport, ReconciliationResult, ReconciliationStorage,
};
use crate::traits::*;
use crate::types::*;

/// In-memory storage implementation for testing and development
#[derive(Debug, Clone)]
pub struct MemoryStorage {
    accounts: Arc<RwLock<HashMap<String, Account>>>,
    transactions: Arc<RwLock<HashMap<String, Transaction>>>,
}

impl MemoryStorage {
    /// Create a new memory storage instance
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            transactions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Clear all data (useful for testing)
    pub fn clear(&self) {
        self.accounts.write().unwrap().clear();
        self.transactions.write().unwrap().clear();
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LedgerStorage for MemoryStorage {
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()> {
        self.accounts
            .write()
            .unwrap()
            .insert(account.id.clone(), account.clone());
        Ok(())
    }

    async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>> {
        Ok(self.accounts.read().unwrap().get(account_id).cloned())
    }

    async fn list_accounts(
        &self,
        account_type: Option<AccountType>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        let accounts = self.accounts.read().unwrap();
        let mut filtered: Vec<Account> = accounts
            .values()
            .filter(|account| {
                account_type
                    .as_ref()
                    .is_none_or(|t| &account.account_type == t)
            })
            .cloned()
            .collect();

        // Sort by account ID for consistent results
        filtered.sort_by(|a, b| a.id.cmp(&b.id));

        match pagination {
            PaginationOption::All => Ok(ListResponse::All(filtered)),
            PaginationOption::Paginated(pagination_params) => {
                let total_count = filtered.len() as u32;
                let start_index = pagination_params.offset() as usize;
                let end_index = std::cmp::min(
                    start_index + pagination_params.limit() as usize,
                    filtered.len(),
                );

                let items = if start_index < filtered.len() {
                    filtered[start_index..end_index].to_vec()
                } else {
                    Vec::new()
                };

                Ok(ListResponse::Paginated(PaginatedResponse::new(
                    items,
                    pagination_params.page,
                    pagination_params.page_size,
                    total_count,
                )))
            }
        }
    }

    async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        let mut accounts = self.accounts.write().unwrap();
        if accounts.contains_key(&account.id) {
            accounts.insert(account.id.clone(), account.clone());
            Ok(())
        } else {
            Err(LedgerError::AccountNotFound(account.id.clone()))
        }
    }

    async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()> {
        if self.accounts.write().unwrap().remove(account_id).is_some() {
            Ok(())
        } else {
            Err(LedgerError::AccountNotFound(account_id.to_string()))
        }
    }

    async fn save_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        self.transactions
            .write()
            .unwrap()
            .insert(transaction.id.clone(), transaction.clone());
        Ok(())
    }

    async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>> {
        Ok(self
            .transactions
            .read()
            .unwrap()
            .get(transaction_id)
            .cloned())
    }

    async fn get_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        let transactions = self.transactions.read().unwrap();
        let mut filtered: Vec<Transaction> = transactions
            .values()
            .filter(|txn| {
                // Check if transaction affects the account
                let affects_account = txn
                    .entries
                    .iter()
                    .any(|entry| entry.account_id == account_id);
                if !affects_account {
                    return false;
                }

                // Check date range
                if let Some(start) = start_date {
                    if txn.date < start {
                        return false;
                    }
                }
                if let Some(end) = end_date {
                    if txn.date > end {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort by date descending, then by ID for consistent results
        filtered.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));

        match pagination {
            PaginationOption::All => Ok(ListResponse::All(filtered)),
            PaginationOption::Paginated(pagination_params) => {
                let total_count = filtered.len() as u32;
                let start_index = pagination_params.offset() as usize;
                let end_index = std::cmp::min(
                    start_index + pagination_params.limit() as usize,
                    filtered.len(),
                );

                let items = if start_index < filtered.len() {
                    filtered[start_index..end_index].to_vec()
                } else {
                    Vec::new()
                };

                Ok(ListResponse::Paginated(PaginatedResponse::new(
                    items,
                    pagination_params.page,
                    pagination_params.page_size,
                    total_count,
                )))
            }
        }
    }

    async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        let transactions = self.transactions.read().unwrap();
        let mut filtered: Vec<Transaction> = transactions
            .values()
            .filter(|txn| {
                if let Some(start) = start_date {
                    if txn.date < start {
                        return false;
                    }
                }
                if let Some(end) = end_date {
                    if txn.date > end {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by date descending, then by ID for consistent results
        filtered.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));

        match pagination {
            PaginationOption::All => Ok(ListResponse::All(filtered)),
            PaginationOption::Paginated(pagination_params) => {
                let total_count = filtered.len() as u32;
                let start_index = pagination_params.offset() as usize;
                let end_index = std::cmp::min(
                    start_index + pagination_params.limit() as usize,
                    filtered.len(),
                );

                let items = if start_index < filtered.len() {
                    filtered[start_index..end_index].to_vec()
                } else {
                    Vec::new()
                };

                Ok(ListResponse::Paginated(PaginatedResponse::new(
                    items,
                    pagination_params.page,
                    pagination_params.page_size,
                    total_count,
                )))
            }
        }
    }

    async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        if self
            .transactions
            .read()
            .unwrap()
            .contains_key(&transaction.id)
        {
            self.transactions
                .write()
                .unwrap()
                .insert(transaction.id.clone(), transaction.clone());
            Ok(())
        } else {
            Err(LedgerError::TransactionNotFound(transaction.id.clone()))
        }
    }

    async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()> {
        if self
            .transactions
            .write()
            .unwrap()
            .remove(transaction_id)
            .is_some()
        {
            Ok(())
        } else {
            Err(LedgerError::TransactionNotFound(transaction_id.to_string()))
        }
    }

    async fn get_account_balance(
        &self,
        account_id: &str,
        as_of_date: Option<NaiveDate>,
    ) -> LedgerResult<BigDecimal> {
        let account = self
            .get_account(account_id)
            .await?
            .ok_or_else(|| LedgerError::AccountNotFound(account_id.to_string()))?;

        // If no date specified, return current balance
        if as_of_date.is_none() {
            return Ok(account.balance);
        }

        // Calculate balance as of specific date
        let mut balance = BigDecimal::from(0);
        let transactions = self
            .get_account_transactions(account_id, None, as_of_date, PaginationOption::All)
            .await?;

        for transaction in transactions.into_items() {
            for entry in transaction.entries {
                if entry.account_id == account_id {
                    match (account.account_type.normal_balance(), entry.entry_type) {
                        (EntryType::Debit, EntryType::Debit)
                        | (EntryType::Credit, EntryType::Credit) => {
                            balance += entry.amount;
                        }
                        (EntryType::Debit, EntryType::Credit)
                        | (EntryType::Credit, EntryType::Debit) => {
                            balance -= entry.amount;
                        }
                    }
                }
            }
        }

        Ok(balance)
    }

    async fn get_trial_balance(&self, as_of_date: NaiveDate) -> LedgerResult<TrialBalance> {
        let accounts = self.list_accounts(None, PaginationOption::All).await?;
        let mut balances = HashMap::new();
        let mut total_debits = BigDecimal::from(0);
        let mut total_credits = BigDecimal::from(0);

        for account in accounts.into_items() {
            let balance = self
                .get_account_balance(&account.id, Some(as_of_date))
                .await?;

            let account_balance = match account.account_type.normal_balance() {
                EntryType::Debit => {
                    if balance >= *crate::ZERO {
                        total_debits += &balance;
                        AccountBalance {
                            account: account.clone(),
                            debit_balance: Some(balance),
                            credit_balance: None,
                        }
                    } else {
                        total_credits += balance.abs();
                        AccountBalance {
                            account: account.clone(),
                            debit_balance: None,
                            credit_balance: Some(balance.abs()),
                        }
                    }
                }
                EntryType::Credit => {
                    if balance >= *crate::ZERO {
                        total_credits += &balance;
                        AccountBalance {
                            account: account.clone(),
                            debit_balance: None,
                            credit_balance: Some(balance),
                        }
                    } else {
                        total_debits += balance.abs();
                        AccountBalance {
                            account: account.clone(),
                            debit_balance: Some(balance.abs()),
                            credit_balance: None,
                        }
                    }
                }
            };

            balances.insert(account.id.clone(), account_balance);
        }

        let is_balanced = total_debits == total_credits;

        Ok(TrialBalance {
            as_of_date,
            balances,
            total_debits,
            total_credits,
            is_balanced,
        })
    }

    async fn get_account_balances_by_type(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<HashMap<AccountType, Vec<AccountBalance>>> {
        let trial_balance = self.get_trial_balance(as_of_date).await?;
        let mut result: HashMap<AccountType, Vec<AccountBalance>> = HashMap::new();

        for account_balance in trial_balance.balances.into_values() {
            let account_type = account_balance.account.account_type;
            result
                .entry(account_type)
                .or_default()
                .push(account_balance);
        }

        Ok(result)
    }
}

/// In-memory reconciliation storage for testing and development
///
/// A reference implementation of [`ReconciliationStorage`] that keeps everything in a lock behind
/// an `Arc`, mirroring [`MemoryStorage`]. Ledger transactions are seeded with
/// [`add_ledger_transaction`](Self::add_ledger_transaction) rather than derived from a ledger, so
/// the reconciliation side can be exercised in isolation.
#[derive(Debug, Clone, Default)]
pub struct MemoryReconciliationStorage {
    ledger_transactions: Arc<RwLock<Vec<LedgerTransaction>>>,
    reports: Arc<RwLock<HashMap<Uuid, ReconciliationReport>>>,
    reconciled: Arc<RwLock<HashMap<String, (String, Uuid)>>>,
}

impl MemoryReconciliationStorage {
    /// Create an empty store
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a ledger transaction leg
    pub fn add_ledger_transaction(&self, transaction: LedgerTransaction) {
        self.ledger_transactions.write().unwrap().push(transaction);
    }

    /// Every reconciliation mark recorded so far, keyed by ledger transaction id
    pub fn reconciled_marks(&self) -> HashMap<String, (String, Uuid)> {
        self.reconciled.read().unwrap().clone()
    }

    /// Clear all data (useful for testing)
    pub fn clear(&self) {
        self.ledger_transactions.write().unwrap().clear();
        self.reports.write().unwrap().clear();
        self.reconciled.write().unwrap().clear();
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
        let mut matching: Vec<LedgerTransaction> = self
            .ledger_transactions
            .read()
            .unwrap()
            .iter()
            .filter(|transaction| {
                transaction.account_id == account_id
                    && transaction.date >= start_date
                    && transaction.date <= end_date
            })
            .cloned()
            .collect();

        matching.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
        Ok(matching)
    }

    async fn save_reconciliation_report(
        &mut self,
        report: &ReconciliationReport,
    ) -> ReconciliationResult<()> {
        self.reports
            .write()
            .unwrap()
            .insert(report.id, report.clone());
        Ok(())
    }

    async fn get_reconciliation_report(
        &self,
        report_id: Uuid,
    ) -> ReconciliationResult<Option<ReconciliationReport>> {
        Ok(self.reports.read().unwrap().get(&report_id).cloned())
    }

    async fn list_reconciliation_reports(
        &self,
        account_id: &str,
        pagination: PaginationOption,
    ) -> ReconciliationResult<ListResponse<ReconciliationReport>> {
        let reports = self.reports.read().unwrap();
        let mut filtered: Vec<ReconciliationReport> = reports
            .values()
            .filter(|report| report.account_ids.iter().any(|id| id == account_id))
            .cloned()
            .collect();

        // Newest first, with the id as a stable tie-breaker
        filtered.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        match pagination {
            PaginationOption::All => Ok(ListResponse::All(filtered)),
            PaginationOption::Paginated(pagination_params) => {
                let total_count = filtered.len() as u32;
                let start_index = pagination_params.offset() as usize;
                let end_index = std::cmp::min(
                    start_index + pagination_params.limit() as usize,
                    filtered.len(),
                );

                let items = if start_index < filtered.len() {
                    filtered[start_index..end_index].to_vec()
                } else {
                    Vec::new()
                };

                Ok(ListResponse::Paginated(PaginatedResponse::new(
                    items,
                    pagination_params.page,
                    pagination_params.page_size,
                    total_count,
                )))
            }
        }
    }

    async fn mark_transaction_as_reconciled(
        &mut self,
        ledger_id: &str,
        external_id: &str,
        reconciliation_id: Uuid,
    ) -> ReconciliationResult<()> {
        self.reconciled.write().unwrap().insert(
            ledger_id.to_string(),
            (external_id.to_string(), reconciliation_id),
        );
        Ok(())
    }
}
