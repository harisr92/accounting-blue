//! The ledger: accounts, transactions, balances and reports over a pluggable store
//!
//! [`Ledger`] is the imperative shell. It loads data from its [`LedgerStorage`], runs the
//! validators and the pure calculations in [`balances`] and [`crate::reports`], and writes the
//! results back.

mod accounts;
pub mod balances;
mod builder;
pub mod patterns;
mod transactions;

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashMap;

use crate::error::LedgerResult;
use crate::reports::{
    self, BalanceSheet, BalancesByType, CashFlowStatement, IncomeStatement, LedgerIntegrityReport,
};
use crate::traits::{AccountValidator, LedgerStorage, TransactionValidator};
use crate::types::{
    Account, AccountType, ListResponse, PaginationOption, Transaction, TransactionFilter,
    TrialBalance,
};
use crate::utils::validation::{DefaultAccountValidator, DefaultTransactionValidator};

pub use accounts::STANDARD_CHART;
pub use builder::TransactionBuilder;
pub use patterns::{BillPaymentWithGstParams, InvoiceWithGstParams};

/// Main ledger system that orchestrates all accounting operations
pub struct Ledger<S: LedgerStorage> {
    storage: S,
    account_validator: Box<dyn AccountValidator>,
    transaction_validator: Box<dyn TransactionValidator>,
}

impl<S: LedgerStorage> Ledger<S> {
    /// Create a ledger with the default validators
    pub fn new(storage: S) -> Self {
        Self::with_validators(
            storage,
            Box::new(DefaultAccountValidator),
            Box::new(DefaultTransactionValidator),
        )
    }

    /// Create a ledger with custom validators
    pub fn with_validators(
        storage: S,
        account_validator: Box<dyn AccountValidator>,
        transaction_validator: Box<dyn TransactionValidator>,
    ) -> Self {
        Self {
            storage,
            account_validator,
            transaction_validator,
        }
    }

    /// The underlying store
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Give back the underlying store
    pub fn into_storage(self) -> S {
        self.storage
    }

    // Accounts

    /// Create a new account
    ///
    /// # Errors
    ///
    /// A validation error, [`LedgerError::DuplicateAccount`](crate::LedgerError::DuplicateAccount),
    /// [`LedgerError::ParentNotFound`](crate::LedgerError::ParentNotFound) or a storage error.
    pub async fn create_account(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        account_type: AccountType,
        parent_id: Option<String>,
    ) -> LedgerResult<Account> {
        let account = Account::new(id, name, account_type, parent_id);
        accounts::create_account(&mut self.storage, &*self.account_validator, account).await
    }

    /// Get an account by ID
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>> {
        self.storage.get_account(account_id).await
    }

    /// List all accounts, sorted by id
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_accounts(
        &self,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        self.storage.list_accounts(None, pagination).await
    }

    /// List accounts of one type, sorted by id
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_accounts_by_type(
        &self,
        account_type: AccountType,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        self.storage
            .list_accounts(Some(account_type), pagination)
            .await
    }

    /// Every account, unpaginated
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_all_accounts(&self) -> LedgerResult<Vec<Account>> {
        Ok(self
            .list_accounts(PaginationOption::All)
            .await?
            .into_items())
    }

    /// Every account of one type, unpaginated
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_all_accounts_by_type(
        &self,
        account_type: AccountType,
    ) -> LedgerResult<Vec<Account>> {
        Ok(self
            .list_accounts_by_type(account_type, PaginationOption::All)
            .await?
            .into_items())
    }

    /// Direct children of an account
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn child_accounts(&self, parent_id: &str) -> LedgerResult<Vec<Account>> {
        accounts::child_accounts(&self.storage, parent_id).await
    }

    /// The chain of accounts from the root of the hierarchy down to `account_id`
    ///
    /// # Errors
    ///
    /// [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound) for a missing link,
    /// [`LedgerError::AccountCycle`](crate::LedgerError::AccountCycle), or a storage error.
    pub async fn account_path(&self, account_id: &str) -> LedgerResult<Vec<Account>> {
        accounts::account_path(&self.storage, account_id).await
    }

    /// Update an account
    ///
    /// # Errors
    ///
    /// A validation error, [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound)
    /// or a storage error.
    pub async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        accounts::update_account(&mut self.storage, &*self.account_validator, account).await
    }

    /// Delete an account that no transaction or child account refers to
    ///
    /// Deleting a used account would leave its transactions pointing at nothing, and reports built
    /// afterwards could no longer tell what the account was. Delete its transactions and move or
    /// delete its children first.
    ///
    /// # Errors
    ///
    /// [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound),
    /// [`LedgerError::AccountHasChildren`](crate::LedgerError::AccountHasChildren),
    /// [`LedgerError::AccountHasTransactions`](crate::LedgerError::AccountHasTransactions) or a
    /// storage error. The account is left in place on any error.
    pub async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()> {
        accounts::delete_account(&mut self.storage, account_id).await
    }

    /// Create the accounts in [`STANDARD_CHART`], keyed by their short name
    ///
    /// # Errors
    ///
    /// The first error from [`Ledger::create_account`].
    pub async fn setup_standard_chart_of_accounts(
        &mut self,
    ) -> LedgerResult<HashMap<String, Account>> {
        accounts::create_standard_chart(&mut self.storage, &*self.account_validator).await
    }

    // Transactions

    /// Validate a transaction, save it and apply it to account balances
    ///
    /// # Errors
    ///
    /// A validation error, [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound)
    /// for an entry, or a storage error.
    pub async fn record_transaction(&mut self, transaction: Transaction) -> LedgerResult<()> {
        transactions::record_transaction(
            &mut self.storage,
            &*self.transaction_validator,
            transaction,
        )
        .await
    }

    /// Get a transaction by ID
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>> {
        self.storage.get_transaction(transaction_id).await
    }

    /// List transactions dated within an inclusive range, newest first
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        let filter = TransactionFilter::between(start_date, end_date);
        self.storage.list_transactions(&filter, pagination).await
    }

    /// List one account's transactions dated within an inclusive range, newest first
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        let filter = TransactionFilter::between(start_date, end_date).for_account(account_id);
        self.storage.list_transactions(&filter, pagination).await
    }

    /// Every transaction within a date range, unpaginated
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_all_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        Ok(self
            .list_transactions(start_date, end_date, PaginationOption::All)
            .await?
            .into_items())
    }

    /// Every transaction for one account within a date range, unpaginated
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn list_all_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        Ok(self
            .list_account_transactions(account_id, start_date, end_date, PaginationOption::All)
            .await?
            .into_items())
    }

    /// Replace a transaction, reversing its old entries and applying the new ones
    ///
    /// # Errors
    ///
    /// [`LedgerError::TransactionNotFound`](crate::LedgerError::TransactionNotFound), a
    /// validation error, [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound) for
    /// an entry on the new version, or a storage error. Accounts on the old version that have
    /// since been deleted are skipped when reversing.
    pub async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        transactions::update_transaction(
            &mut self.storage,
            &*self.transaction_validator,
            transaction,
        )
        .await
    }

    /// Delete a transaction and reverse its effect on the accounts that still exist
    ///
    /// # Errors
    ///
    /// [`LedgerError::TransactionNotFound`](crate::LedgerError::TransactionNotFound) or a storage
    /// error.
    pub async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()> {
        transactions::delete_transaction(&mut self.storage, transaction_id).await
    }

    // Balances and reports

    /// Account balance: the stored running balance, or recomputed from history as of a date
    ///
    /// # Errors
    ///
    /// [`LedgerError::AccountNotFound`](crate::LedgerError::AccountNotFound) or a storage error.
    pub async fn get_account_balance(
        &self,
        account_id: &str,
        as_of_date: Option<NaiveDate>,
    ) -> LedgerResult<BigDecimal> {
        let account = accounts::require_account(&self.storage, account_id).await?;
        if as_of_date.is_none() {
            return Ok(account.balance);
        }

        let history = self
            .list_all_account_transactions(account_id, None, as_of_date)
            .await?;
        Ok(balances::account_balance(&account, &history))
    }

    /// Trial balance as of a date
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn get_trial_balance(&self, as_of_date: NaiveDate) -> LedgerResult<TrialBalance> {
        let accounts = self.list_all_accounts().await?;
        let history = self.list_all_transactions(None, Some(as_of_date)).await?;
        Ok(balances::trial_balance(as_of_date, accounts, &history))
    }

    /// Account balances as of a date, grouped by account type
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn get_account_balances_by_type(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<BalancesByType> {
        let trial_balance = self.get_trial_balance(as_of_date).await?;
        Ok(balances::group_by_type(
            trial_balance.balances.into_values(),
        ))
    }

    /// Balance sheet as of a date
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn generate_balance_sheet(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<BalanceSheet> {
        let by_type = self.get_account_balances_by_type(as_of_date).await?;
        Ok(reports::balance_sheet(as_of_date, by_type))
    }

    /// Income statement for a period (balances are cumulative to `end_date`)
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn generate_income_statement(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<IncomeStatement> {
        let by_type = self.get_account_balances_by_type(end_date).await?;
        Ok(reports::income_statement(start_date, end_date, by_type))
    }

    /// Simplified cash flow statement for a period, classified by
    /// [`reports::classify_cash_flow`]
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn generate_cash_flow(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<CashFlowStatement> {
        let period = self
            .list_all_transactions(Some(start_date), Some(end_date))
            .await?;
        let accounts: HashMap<String, Account> = self
            .list_all_accounts()
            .await?
            .into_iter()
            .map(|account| (account.id.clone(), account))
            .collect();
        Ok(reports::cash_flow(start_date, end_date, &period, &accounts))
    }

    /// Check that the trial balance and balance sheet both balance
    ///
    /// # Errors
    ///
    /// A storage error.
    pub async fn validate_integrity(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<LedgerIntegrityReport> {
        let trial_balance = self.get_trial_balance(as_of_date).await?;
        let by_type = balances::group_by_type(trial_balance.balances.values().cloned());
        let balance_sheet = reports::balance_sheet(as_of_date, by_type);
        Ok(reports::integrity_report(
            as_of_date,
            trial_balance,
            balance_sheet,
        ))
    }
}

#[cfg(test)]
mod tests;
