//! Account management functionality

use bigdecimal::BigDecimal;
use std::collections::HashMap;

use crate::traits::*;
use crate::types::*;

/// Account manager for handling chart of accounts operations
pub struct AccountManager<S: LedgerStorage> {
    pub(crate) storage: S,
    validator: Box<dyn AccountValidator>,
}

impl<S: LedgerStorage> AccountManager<S> {
    /// Create a new account manager
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            validator: Box::new(DefaultAccountValidator),
        }
    }

    /// Create a new account manager with custom validator
    pub fn with_validator(storage: S, validator: Box<dyn AccountValidator>) -> Self {
        Self { storage, validator }
    }

    /// Create a new account
    pub async fn create_account(
        &mut self,
        id: String,
        name: String,
        account_type: AccountType,
        parent_id: Option<String>,
    ) -> LedgerResult<Account> {
        let account = Account::new(id, name, account_type, parent_id);

        // Validate the account
        self.validator.validate_account(&account)?;

        // Check if account already exists
        if let Some(_existing) = self.storage.get_account(&account.id).await? {
            return Err(LedgerError::Validation(format!(
                "Account with ID '{}' already exists",
                account.id
            )));
        }

        // Validate parent account exists if specified
        if let Some(ref parent_id) = account.parent_id {
            if self.storage.get_account(parent_id).await?.is_none() {
                return Err(LedgerError::Validation(format!(
                    "Parent account '{}' does not exist",
                    parent_id
                )));
            }
        }

        // Save the account
        self.storage.save_account(&account).await?;

        Ok(account)
    }

    /// Get an account by ID
    pub async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>> {
        self.storage.get_account(account_id).await
    }

    /// Get an account by ID, returning an error if not found
    pub async fn get_account_required(&self, account_id: &str) -> LedgerResult<Account> {
        self.storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| LedgerError::AccountNotFound(account_id.to_string()))
    }

    /// List all accounts
    pub async fn list_accounts(&self) -> LedgerResult<Vec<Account>> {
        self.storage.list_accounts(None).await
    }

    /// List accounts by type
    pub async fn list_accounts_by_type(
        &self,
        account_type: AccountType,
    ) -> LedgerResult<Vec<Account>> {
        self.storage.list_accounts(Some(account_type)).await
    }

    /// Update an account
    pub async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        // Validate the account
        self.validator.validate_account(account)?;

        // Ensure the account exists
        if self.storage.get_account(&account.id).await?.is_none() {
            return Err(LedgerError::AccountNotFound(account.id.clone()));
        }

        self.storage.update_account(account).await
    }

    /// Delete an account
    pub async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()> {
        // Validate deletion
        self.validator.validate_account_deletion(account_id)?;

        // Ensure the account exists
        if self.storage.get_account(account_id).await?.is_none() {
            return Err(LedgerError::AccountNotFound(account_id.to_string()));
        }

        self.storage.delete_account(account_id).await
    }

    /// Get account balance
    pub async fn get_balance(
        &self,
        account_id: &str,
        as_of_date: Option<chrono::NaiveDate>,
    ) -> LedgerResult<BigDecimal> {
        self.storage
            .get_account_balance(account_id, as_of_date)
            .await
    }
}

/// Chart of accounts implementation
pub struct StandardChartOfAccounts<S: LedgerStorage> {
    account_manager: AccountManager<S>,
}

impl<S: LedgerStorage> StandardChartOfAccounts<S> {
    /// Create a new chart of accounts
    pub fn new(storage: S) -> Self {
        Self {
            account_manager: AccountManager::new(storage),
        }
    }
}

#[async_trait::async_trait]
impl<S: LedgerStorage> ChartOfAccounts for StandardChartOfAccounts<S> {
    async fn get_chart(&self) -> LedgerResult<Vec<Account>> {
        self.account_manager.list_accounts().await
    }

    async fn add_account(&mut self, account: Account) -> LedgerResult<()> {
        self.account_manager.storage.save_account(&account).await
    }

    async fn get_child_accounts(&self, parent_id: &str) -> LedgerResult<Vec<Account>> {
        let all_accounts = self.account_manager.list_accounts().await?;
        Ok(all_accounts
            .into_iter()
            .filter(|account| account.parent_id.as_deref() == Some(parent_id))
            .collect())
    }

    async fn get_account_path(&self, account_id: &str) -> LedgerResult<Vec<Account>> {
        let mut path = Vec::new();
        let mut current_account_id = Some(account_id.to_string());

        while let Some(id) = current_account_id {
            match self.account_manager.get_account(&id).await? {
                Some(account) => {
                    current_account_id = account.parent_id.clone();
                    path.insert(0, account);
                }
                None => {
                    return Err(LedgerError::AccountNotFound(id));
                }
            }
        }

        Ok(path)
    }
}

/// Utility functions for working with accounts
pub mod utils {
    use super::*;

    /// Create a standard chart of accounts for a small business
    pub async fn create_standard_chart<S: LedgerStorage>(
        account_manager: &mut AccountManager<S>,
    ) -> LedgerResult<HashMap<String, Account>> {
        let mut accounts = HashMap::new();

        // Assets
        let cash = account_manager
            .create_account(
                "1000".to_string(),
                "Cash".to_string(),
                AccountType::Asset,
                None,
            )
            .await?;
        accounts.insert("cash".to_string(), cash);

        let accounts_receivable = account_manager
            .create_account(
                "1200".to_string(),
                "Accounts Receivable".to_string(),
                AccountType::Asset,
                None,
            )
            .await?;
        accounts.insert("accounts_receivable".to_string(), accounts_receivable);

        let inventory = account_manager
            .create_account(
                "1300".to_string(),
                "Inventory".to_string(),
                AccountType::Asset,
                None,
            )
            .await?;
        accounts.insert("inventory".to_string(), inventory);

        // Liabilities
        let accounts_payable = account_manager
            .create_account(
                "2000".to_string(),
                "Accounts Payable".to_string(),
                AccountType::Liability,
                None,
            )
            .await?;
        accounts.insert("accounts_payable".to_string(), accounts_payable);

        let loans_payable = account_manager
            .create_account(
                "2100".to_string(),
                "Loans Payable".to_string(),
                AccountType::Liability,
                None,
            )
            .await?;
        accounts.insert("loans_payable".to_string(), loans_payable);

        // Equity
        let owners_equity = account_manager
            .create_account(
                "3000".to_string(),
                "Owner's Equity".to_string(),
                AccountType::Equity,
                None,
            )
            .await?;
        accounts.insert("owners_equity".to_string(), owners_equity);

        let retained_earnings = account_manager
            .create_account(
                "3200".to_string(),
                "Retained Earnings".to_string(),
                AccountType::Equity,
                None,
            )
            .await?;
        accounts.insert("retained_earnings".to_string(), retained_earnings);

        // Income
        let sales_revenue = account_manager
            .create_account(
                "4000".to_string(),
                "Sales Revenue".to_string(),
                AccountType::Income,
                None,
            )
            .await?;
        accounts.insert("sales_revenue".to_string(), sales_revenue);

        let service_revenue = account_manager
            .create_account(
                "4100".to_string(),
                "Service Revenue".to_string(),
                AccountType::Income,
                None,
            )
            .await?;
        accounts.insert("service_revenue".to_string(), service_revenue);

        // Expenses
        let cost_of_goods_sold = account_manager
            .create_account(
                "5000".to_string(),
                "Cost of Goods Sold".to_string(),
                AccountType::Expense,
                None,
            )
            .await?;
        accounts.insert("cost_of_goods_sold".to_string(), cost_of_goods_sold);

        let rent_expense = account_manager
            .create_account(
                "6000".to_string(),
                "Rent Expense".to_string(),
                AccountType::Expense,
                None,
            )
            .await?;
        accounts.insert("rent_expense".to_string(), rent_expense);

        let utilities_expense = account_manager
            .create_account(
                "6100".to_string(),
                "Utilities Expense".to_string(),
                AccountType::Expense,
                None,
            )
            .await?;
        accounts.insert("utilities_expense".to_string(), utilities_expense);

        Ok(accounts)
    }
}
