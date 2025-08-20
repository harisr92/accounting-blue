//! Traits for storage abstraction and extensibility

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::*;

/// Storage abstraction for the ledger system
///
/// This trait allows the accounting core to work with any storage backend
/// (PostgreSQL, MySQL, SQLite, in-memory, etc.) by implementing these methods.
#[async_trait]
pub trait LedgerStorage: Send + Sync {
    /// Save an account to storage
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()>;

    /// Get an account by ID
    async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>>;

    /// List all accounts, optionally filtered by type
    async fn list_accounts(&self, account_type: Option<AccountType>) -> LedgerResult<Vec<Account>>;

    /// Update an account
    async fn update_account(&mut self, account: &Account) -> LedgerResult<()>;

    /// Delete an account (if no transactions reference it)
    async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()>;

    /// Save a transaction to storage
    async fn save_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()>;

    /// Get a transaction by ID
    async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>>;

    /// List transactions for a specific account
    async fn get_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>>;

    /// List all transactions within a date range
    async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>>;

    /// Update a transaction
    async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()>;

    /// Delete a transaction and reverse its effects on account balances
    async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()>;

    /// Get account balance as of a specific date
    async fn get_account_balance(
        &self,
        account_id: &str,
        as_of_date: Option<NaiveDate>,
    ) -> LedgerResult<BigDecimal>;

    /// Get trial balance as of a specific date
    async fn get_trial_balance(&self, as_of_date: NaiveDate) -> LedgerResult<TrialBalance>;

    /// Get all account balances grouped by account type
    async fn get_account_balances_by_type(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<HashMap<AccountType, Vec<AccountBalance>>>;
}

/// Trait for implementing custom account validation rules
pub trait AccountValidator: Send + Sync {
    /// Validate an account before saving
    fn validate_account(&self, account: &Account) -> LedgerResult<()>;

    /// Validate account deletion (e.g., check for existing transactions)
    fn validate_account_deletion(&self, account_id: &str) -> LedgerResult<()>;
}

/// Trait for implementing custom transaction validation rules
pub trait TransactionValidator: Send + Sync {
    /// Validate a transaction before saving
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()>;

    /// Validate that all referenced accounts exist
    fn validate_account_references(&self, transaction: &Transaction) -> LedgerResult<()>;
}

/// Default account validator with basic rules
pub struct DefaultAccountValidator;

impl AccountValidator for DefaultAccountValidator {
    fn validate_account(&self, account: &Account) -> LedgerResult<()> {
        if account.id.trim().is_empty() {
            return Err(LedgerError::Validation(
                "Account ID cannot be empty".to_string(),
            ));
        }

        if account.name.trim().is_empty() {
            return Err(LedgerError::Validation(
                "Account name cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_account_deletion(&self, _account_id: &str) -> LedgerResult<()> {
        // Basic implementation - in a real system you'd check for existing transactions
        Ok(())
    }
}

/// Default transaction validator with basic double-entry rules
pub struct DefaultTransactionValidator;

impl TransactionValidator for DefaultTransactionValidator {
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()> {
        transaction.validate()
    }

    fn validate_account_references(&self, _transaction: &Transaction) -> LedgerResult<()> {
        // Basic implementation - in a real system you'd verify accounts exist in storage
        Ok(())
    }
}

/// Trait for implementing custom chart of accounts structures
#[async_trait]
pub trait ChartOfAccounts: Send + Sync {
    /// Get the full chart of accounts as a hierarchical structure
    async fn get_chart(&self) -> LedgerResult<Vec<Account>>;

    /// Add a new account to the chart
    async fn add_account(&mut self, account: Account) -> LedgerResult<()>;

    /// Get all child accounts of a parent account
    async fn get_child_accounts(&self, parent_id: &str) -> LedgerResult<Vec<Account>>;

    /// Get the full path to an account (for hierarchical display)
    async fn get_account_path(&self, account_id: &str) -> LedgerResult<Vec<Account>>;
}

/// Trait for report generation
#[async_trait]
pub trait ReportGenerator: Send + Sync {
    /// Generate a balance sheet as of a specific date
    async fn generate_balance_sheet(&self, as_of_date: NaiveDate) -> LedgerResult<BalanceSheet>;

    /// Generate an income statement for a date range
    async fn generate_income_statement(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<IncomeStatement>;

    /// Generate a cash flow statement for a date range
    async fn generate_cash_flow(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<CashFlowStatement>;
}

/// Balance Sheet structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceSheet {
    pub as_of_date: NaiveDate,
    pub assets: Vec<AccountBalance>,
    pub liabilities: Vec<AccountBalance>,
    pub equity: Vec<AccountBalance>,
    pub total_assets: BigDecimal,
    pub total_liabilities: BigDecimal,
    pub total_equity: BigDecimal,
    pub is_balanced: bool,
}

/// Income Statement structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncomeStatement {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub revenue: Vec<AccountBalance>,
    pub expenses: Vec<AccountBalance>,
    pub total_revenue: BigDecimal,
    pub total_expenses: BigDecimal,
    pub net_income: BigDecimal,
}

/// Cash Flow Statement structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlowStatement {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub operating_activities: Vec<CashFlowItem>,
    pub investing_activities: Vec<CashFlowItem>,
    pub financing_activities: Vec<CashFlowItem>,
    pub net_operating_cash_flow: BigDecimal,
    pub net_investing_cash_flow: BigDecimal,
    pub net_financing_cash_flow: BigDecimal,
    pub net_cash_flow: BigDecimal,
}

/// Cash Flow Item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlowItem {
    pub description: String,
    pub amount: BigDecimal,
}
