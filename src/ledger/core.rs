//! Main ledger orchestrator that coordinates accounts and transactions

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ledger::{AccountManager, TransactionManager};
use crate::traits::*;
use crate::types::*;

/// Main ledger system that orchestrates all accounting operations
pub struct Ledger<S: LedgerStorage> {
    account_manager: AccountManager<S>,
    transaction_manager: TransactionManager<S>,
}

impl<S: LedgerStorage + Clone> Ledger<S> {
    /// Create a new ledger with the given storage backend
    pub fn new(storage: S) -> Self {
        Self {
            account_manager: AccountManager::new(storage.clone()),
            transaction_manager: TransactionManager::new(storage),
        }
    }

    /// Create a new ledger with custom validators
    pub fn with_validators(
        storage: S,
        account_validator: Box<dyn AccountValidator>,
        transaction_validator: Box<dyn TransactionValidator>,
    ) -> Self {
        Self {
            account_manager: AccountManager::with_validator(storage.clone(), account_validator),
            transaction_manager: TransactionManager::with_validator(storage, transaction_validator),
        }
    }

    // Account operations
    /// Create a new account
    pub async fn create_account(
        &mut self,
        id: String,
        name: String,
        account_type: AccountType,
        parent_id: Option<String>,
    ) -> LedgerResult<Account> {
        self.account_manager
            .create_account(id, name, account_type, parent_id)
            .await
    }

    /// Get an account by ID
    pub async fn get_account(&self, account_id: &str) -> LedgerResult<Option<Account>> {
        self.account_manager.get_account(account_id).await
    }

    /// List all accounts
    pub async fn list_accounts(&self) -> LedgerResult<Vec<Account>> {
        self.account_manager.list_accounts().await
    }

    /// List accounts by type
    pub async fn list_accounts_by_type(
        &self,
        account_type: AccountType,
    ) -> LedgerResult<Vec<Account>> {
        self.account_manager
            .list_accounts_by_type(account_type)
            .await
    }

    /// Update an account
    pub async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        self.account_manager.update_account(account).await
    }

    /// Delete an account
    pub async fn delete_account(&mut self, account_id: &str) -> LedgerResult<()> {
        self.account_manager.delete_account(account_id).await
    }

    // Transaction operations
    /// Record a new transaction
    pub async fn record_transaction(&mut self, transaction: Transaction) -> LedgerResult<()> {
        self.transaction_manager
            .record_transaction(transaction)
            .await
    }

    /// Get a transaction by ID
    pub async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>> {
        self.transaction_manager
            .get_transaction(transaction_id)
            .await
    }

    /// Get transactions for a specific account
    pub async fn get_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        self.transaction_manager
            .get_account_transactions(account_id, start_date, end_date)
            .await
    }

    /// Get all transactions within a date range
    pub async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        self.transaction_manager
            .get_transactions(start_date, end_date)
            .await
    }

    /// Update a transaction
    pub async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        self.transaction_manager
            .update_transaction(transaction)
            .await
    }

    /// Delete a transaction
    pub async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()> {
        self.transaction_manager
            .delete_transaction(transaction_id)
            .await
    }

    // Balance and reporting operations
    /// Get account balance as of a specific date
    pub async fn get_account_balance(
        &self,
        account_id: &str,
        as_of_date: Option<NaiveDate>,
    ) -> LedgerResult<BigDecimal> {
        self.account_manager
            .get_balance(account_id, as_of_date)
            .await
    }

    /// Get trial balance as of a specific date
    pub async fn get_trial_balance(&self, as_of_date: NaiveDate) -> LedgerResult<TrialBalance> {
        self.account_manager
            .storage
            .get_trial_balance(as_of_date)
            .await
    }

    /// Get account balances grouped by type
    pub async fn get_account_balances_by_type(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<HashMap<AccountType, Vec<AccountBalance>>> {
        self.account_manager
            .storage
            .get_account_balances_by_type(as_of_date)
            .await
    }

    /// Generate a balance sheet as of a specific date
    pub async fn generate_balance_sheet(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<BalanceSheet> {
        let balances = self.get_account_balances_by_type(as_of_date).await?;

        let assets = balances
            .get(&AccountType::Asset)
            .cloned()
            .unwrap_or_default();
        let liabilities = balances
            .get(&AccountType::Liability)
            .cloned()
            .unwrap_or_default();
        let mut equity = balances
            .get(&AccountType::Equity)
            .cloned()
            .unwrap_or_default();

        // Calculate net income from revenue and expenses
        let income_accounts = balances
            .get(&AccountType::Income)
            .cloned()
            .unwrap_or_default();
        let expense_accounts = balances
            .get(&AccountType::Expense)
            .cloned()
            .unwrap_or_default();

        let total_income: BigDecimal = income_accounts.iter().map(|ab| ab.balance_amount()).sum();
        let total_expenses: BigDecimal =
            expense_accounts.iter().map(|ab| ab.balance_amount()).sum();
        let net_income = &total_income - &total_expenses;

        // Add net income to equity as retained earnings (if non-zero)
        if net_income != BigDecimal::from(0) {
            let retained_earnings = AccountBalance {
                account: Account::new(
                    "net_income".to_string(),
                    "Net Income".to_string(),
                    AccountType::Equity,
                    None,
                ),
                debit_balance: if net_income < BigDecimal::from(0) {
                    Some(net_income.abs())
                } else {
                    None
                },
                credit_balance: if net_income > BigDecimal::from(0) {
                    Some(net_income)
                } else {
                    None
                },
            };
            equity.push(retained_earnings);
        }

        let total_assets: BigDecimal = assets.iter().map(|ab| ab.balance_amount()).sum();
        let total_liabilities: BigDecimal = liabilities.iter().map(|ab| ab.balance_amount()).sum();
        let total_equity: BigDecimal = equity.iter().map(|ab| ab.balance_amount()).sum();

        let is_balanced = total_assets == (&total_liabilities + &total_equity);

        Ok(BalanceSheet {
            as_of_date,
            assets,
            liabilities,
            equity,
            total_assets,
            total_liabilities,
            total_equity,
            is_balanced,
        })
    }

    /// Generate an income statement for a date range
    pub async fn generate_income_statement(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<IncomeStatement> {
        let balances = self.get_account_balances_by_type(end_date).await?;

        let revenue = balances
            .get(&AccountType::Income)
            .cloned()
            .unwrap_or_default();
        let expenses = balances
            .get(&AccountType::Expense)
            .cloned()
            .unwrap_or_default();

        let total_revenue: BigDecimal = revenue.iter().map(|ab| ab.balance_amount()).sum();
        let total_expenses: BigDecimal = expenses.iter().map(|ab| ab.balance_amount()).sum();
        let net_income = &total_revenue - &total_expenses;

        Ok(IncomeStatement {
            start_date,
            end_date,
            revenue,
            expenses,
            total_revenue,
            total_expenses,
            net_income,
        })
    }

    /// Create a basic cash flow statement
    pub async fn generate_cash_flow(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> LedgerResult<CashFlowStatement> {
        // This is a simplified implementation - a full cash flow statement
        // would require more sophisticated analysis of transaction types

        let transactions = self
            .get_transactions(Some(start_date), Some(end_date))
            .await?;

        let mut operating_activities = Vec::new();
        let mut investing_activities = Vec::new();
        let mut financing_activities = Vec::new();

        // Simplified categorization based on account types involved
        for transaction in transactions {
            let has_asset = transaction.entries.iter().any(|e| {
                // This would need to be enhanced to check actual account types
                e.account_id.contains("asset") || e.account_id.contains("cash")
            });

            let has_liability = transaction
                .entries
                .iter()
                .any(|e| e.account_id.contains("payable") || e.account_id.contains("loan"));

            let has_equity = transaction
                .entries
                .iter()
                .any(|e| e.account_id.contains("equity") || e.account_id.contains("capital"));

            let cash_flow_item = CashFlowItem {
                description: transaction.description.clone(),
                amount: transaction.total_debits(), // Simplified - would need better logic
            };

            if has_equity || has_liability {
                financing_activities.push(cash_flow_item);
            } else if has_asset && transaction.description.to_lowercase().contains("equipment") {
                investing_activities.push(cash_flow_item);
            } else {
                operating_activities.push(cash_flow_item);
            }
        }

        let net_operating_cash_flow: BigDecimal =
            operating_activities.iter().map(|i| &i.amount).sum();
        let net_investing_cash_flow: BigDecimal =
            investing_activities.iter().map(|i| &i.amount).sum();
        let net_financing_cash_flow: BigDecimal =
            financing_activities.iter().map(|i| &i.amount).sum();
        let net_cash_flow =
            &net_operating_cash_flow + &net_investing_cash_flow + &net_financing_cash_flow;

        Ok(CashFlowStatement {
            start_date,
            end_date,
            operating_activities,
            investing_activities,
            financing_activities,
            net_operating_cash_flow,
            net_investing_cash_flow,
            net_financing_cash_flow,
            net_cash_flow,
        })
    }

    /// Setup a standard chart of accounts for small business
    pub async fn setup_standard_chart_of_accounts(
        &mut self,
    ) -> LedgerResult<HashMap<String, Account>> {
        crate::ledger::account::utils::create_standard_chart(&mut self.account_manager).await
    }

    /// Validate the integrity of the ledger
    pub async fn validate_integrity(
        &self,
        as_of_date: NaiveDate,
    ) -> LedgerResult<LedgerIntegrityReport> {
        let trial_balance = self.get_trial_balance(as_of_date).await?;
        let balance_sheet = self.generate_balance_sheet(as_of_date).await?;

        let mut issues = Vec::new();

        // Check if trial balance is balanced
        if !trial_balance.is_balanced {
            issues.push(format!(
                "Trial balance is not balanced: debits = {}, credits = {}",
                trial_balance.total_debits, trial_balance.total_credits
            ));
        }

        let total_liabilities_equity =
            &balance_sheet.total_liabilities + &balance_sheet.total_equity;

        // Check if balance sheet is balanced
        if !balance_sheet.is_balanced {
            issues.push(format!(
                "Balance sheet is not balanced: assets = {}, liabilities + equity = {}",
                balance_sheet.total_assets, total_liabilities_equity
            ));
        }

        // Additional checks could be added here

        Ok(LedgerIntegrityReport {
            as_of_date,
            is_valid: issues.is_empty(),
            issues,
            trial_balance_total_debits: trial_balance.total_debits,
            trial_balance_total_credits: trial_balance.total_credits,
            balance_sheet_total_assets: balance_sheet.total_assets,
            balance_sheet_total_liabilities_equity: total_liabilities_equity,
        })
    }
}

/// Report on ledger integrity and validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerIntegrityReport {
    pub as_of_date: NaiveDate,
    pub is_valid: bool,
    pub issues: Vec<String>,
    pub trial_balance_total_debits: BigDecimal,
    pub trial_balance_total_credits: BigDecimal,
    pub balance_sheet_total_assets: BigDecimal,
    pub balance_sheet_total_liabilities_equity: BigDecimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::memory_storage::MemoryStorage;

    #[tokio::test]
    async fn test_ledger_basic_operations() {
        let storage = MemoryStorage::new();
        let mut ledger = Ledger::new(storage);

        // Create accounts
        let cash_account = ledger
            .create_account(
                "cash".to_string(),
                "Cash".to_string(),
                AccountType::Asset,
                None,
            )
            .await
            .unwrap();

        let revenue_account = ledger
            .create_account(
                "revenue".to_string(),
                "Revenue".to_string(),
                AccountType::Income,
                None,
            )
            .await
            .unwrap();

        // Create a transaction
        let transaction = crate::ledger::transaction::patterns::create_sales_transaction(
            "txn1".to_string(),
            chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            "Sale of goods".to_string(),
            cash_account.id.clone(),
            revenue_account.id.clone(),
            BigDecimal::from(1000),
        )
        .unwrap();

        // Record the transaction
        ledger.record_transaction(transaction).await.unwrap();

        // Check balances
        let cash_balance = ledger
            .get_account_balance(&cash_account.id, None)
            .await
            .unwrap();
        let revenue_balance = ledger
            .get_account_balance(&revenue_account.id, None)
            .await
            .unwrap();

        assert_eq!(cash_balance, BigDecimal::from(1000));
        assert_eq!(revenue_balance, BigDecimal::from(1000));

        // Generate reports
        let balance_sheet = ledger
            .generate_balance_sheet(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            .await
            .unwrap();

        assert_eq!(balance_sheet.total_assets, BigDecimal::from(1000));
    }
}
