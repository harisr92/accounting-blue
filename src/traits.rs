//! Ports: storage and validation traits the ledger depends on
//!
//! Storage traits only move data in and out. Balances, trial balances and reports are computed by
//! the pure functions in [`crate::ledger::balances`] and [`crate::reports`], so a new backend only
//! has to implement plain reads and writes.

use async_trait::async_trait;

use crate::error::LedgerResult;
use crate::types::{
    Account, AccountType, ListResponse, PaginationOption, Transaction, TransactionFilter,
};

/// Persistence for accounts
#[async_trait]
pub trait AccountStore: Send + Sync {
    /// Insert an account, replacing any account with the same id
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()>;

    /// Get an account by id
    async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>>;

    /// List accounts sorted by id, optionally only those of one type
    async fn list_accounts(
        &self,
        account_type: Option<AccountType>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>>;

    /// Replace an existing account
    ///
    /// Fails with [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound) when no
    /// account has this id.
    async fn update_account(&mut self, account: &Account) -> LedgerResult<()>;

    /// Delete an account
    async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()>;
}

/// Persistence for transactions
#[async_trait]
pub trait TransactionStore: Send + Sync {
    /// Insert a transaction, replacing any transaction with the same id
    async fn save_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()>;

    /// Get a transaction by id
    async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>>;

    /// List the transactions that pass `filter`, newest first with the id as a tie-breaker
    async fn list_transactions(
        &self,
        filter: &TransactionFilter,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>>;

    /// Replace an existing transaction
    ///
    /// Fails with [`LedgerError::TransactionNotFound`](crate::LedgerError::TransactionNotFound)
    /// when no transaction has this id.
    async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()>;

    /// Delete a transaction record (balances are the ledger's concern, not the store's)
    async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()>;
}

/// Everything the [`Ledger`](crate::Ledger) needs from a storage backend
///
/// Implemented automatically for any type that is both an [`AccountStore`] and a
/// [`TransactionStore`].
pub trait LedgerStorage: AccountStore + TransactionStore {}

impl<T: AccountStore + TransactionStore> LedgerStorage for T {}

/// Custom account validation rules, run before an account is created or updated
pub trait AccountValidator: Send + Sync {
    /// Validate an account before saving
    ///
    /// # Errors
    ///
    /// The first rule the account breaks.
    fn validate_account(&self, account: &Account) -> LedgerResult<()>;
}

/// Custom transaction validation rules, run before a transaction is recorded or updated
pub trait TransactionValidator: Send + Sync {
    /// Validate a transaction before saving
    ///
    /// # Errors
    ///
    /// The first rule the transaction breaks.
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()>;
}
