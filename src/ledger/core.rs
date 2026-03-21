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

    /// List all accounts with optional pagination
    pub async fn list_accounts(
        &self,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        self.account_manager.list_accounts(pagination).await
    }

    /// List accounts by type with optional pagination
    pub async fn list_accounts_by_type(
        &self,
        account_type: AccountType,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Account>> {
        self.account_manager
            .list_accounts_by_type(account_type, pagination)
            .await
    }

    /// Convenience method: List all accounts without pagination
    pub async fn list_all_accounts(&self) -> LedgerResult<Vec<Account>> {
        let response = self
            .account_manager
            .list_accounts(PaginationOption::All)
            .await?;
        Ok(response.into_items())
    }

    /// Convenience method: List all accounts by type without pagination
    pub async fn list_all_accounts_by_type(
        &self,
        account_type: AccountType,
    ) -> LedgerResult<Vec<Account>> {
        let response = self
            .account_manager
            .list_accounts_by_type(account_type, PaginationOption::All)
            .await?;
        Ok(response.into_items())
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

    /// Get transactions for a specific account with optional pagination
    pub async fn get_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        self.transaction_manager
            .get_account_transactions(account_id, start_date, end_date, pagination)
            .await
    }

    /// Get all transactions within a date range with optional pagination
    pub async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        pagination: PaginationOption,
    ) -> LedgerResult<ListResponse<Transaction>> {
        self.transaction_manager
            .get_transactions(start_date, end_date, pagination)
            .await
    }

    /// Convenience method: Get all transactions for a specific account
    pub async fn get_all_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        let response = self
            .transaction_manager
            .get_account_transactions(account_id, start_date, end_date, PaginationOption::All)
            .await?;
        Ok(response.into_items())
    }

    /// Convenience method: Get all transactions within a date range
    pub async fn get_all_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        let response = self
            .transaction_manager
            .get_transactions(start_date, end_date, PaginationOption::All)
            .await?;
        Ok(response.into_items())
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
        if net_income != *crate::ZERO {
            let retained_earnings = AccountBalance {
                account: Account::new(
                    "net_income".to_string(),
                    "Net Income".to_string(),
                    AccountType::Equity,
                    None,
                ),
                debit_balance: if net_income < *crate::ZERO {
                    Some(net_income.abs())
                } else {
                    None
                },
                credit_balance: if net_income > *crate::ZERO {
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
            .get_transactions(Some(start_date), Some(end_date), PaginationOption::All)
            .await?;

        let mut operating_activities = Vec::new();
        let mut investing_activities = Vec::new();
        let mut financing_activities = Vec::new();

        // Simplified categorization based on account types involved
        for transaction in transactions.into_items() {
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

    #[tokio::test]
    async fn test_unified_accounts_listing() {
        let storage = MemoryStorage::new();
        let mut ledger = Ledger::new(storage);

        // Create multiple accounts
        for i in 1..=25 {
            ledger
                .create_account(
                    format!("account_{}", i),
                    format!("Test Account {}", i),
                    AccountType::Asset,
                    None,
                )
                .await
                .unwrap();
        }

        // Test getting all accounts without pagination
        let all_result = ledger.list_accounts(PaginationOption::All).await.unwrap();
        assert_eq!(all_result.items().len(), 25);
        assert!(!all_result.is_paginated());

        // Test pagination with default page size using unified API
        let pagination = PaginationParams::default(); // page: 1, page_size: 50
        let result = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        let result = result.to_paginated_response();

        assert_eq!(result.items.len(), 25); // All accounts fit in one page
        assert_eq!(result.total_count, 25);
        assert_eq!(result.page, 1);
        assert_eq!(result.page_size, 50);
        assert_eq!(result.total_pages, 1);
        assert!(!result.has_next);
        assert!(!result.has_previous);

        // Test unified API with paginated option
        let pagination_option = PaginationOption::Paginated(PaginationParams::new(1, 10).unwrap());
        let unified_result = ledger.list_accounts(pagination_option).await.unwrap();
        assert!(unified_result.is_paginated());
        assert_eq!(unified_result.items().len(), 10);

        // Test pagination with smaller page size using unified API
        let pagination = PaginationParams::new(1, 10).unwrap();
        let page1_response = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        let page1 = page1_response.to_paginated_response();

        assert_eq!(page1.items.len(), 10);
        assert_eq!(page1.total_count, 25);
        assert_eq!(page1.page, 1);
        assert_eq!(page1.page_size, 10);
        assert_eq!(page1.total_pages, 3);
        assert!(page1.has_next);
        assert!(!page1.has_previous);

        // Test second page
        let pagination = PaginationParams::new(2, 10).unwrap();
        let page2_response = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        let page2 = page2_response.to_paginated_response();

        assert_eq!(page2.items.len(), 10);
        assert_eq!(page2.total_count, 25);
        assert_eq!(page2.page, 2);
        assert!(page2.has_next);
        assert!(page2.has_previous);

        // Test last page
        let pagination = PaginationParams::new(3, 10).unwrap();
        let page3_response = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        let page3 = page3_response.to_paginated_response();

        assert_eq!(page3.items.len(), 5); // Remaining accounts
        assert_eq!(page3.total_count, 25);
        assert_eq!(page3.page, 3);
        assert!(!page3.has_next);
        assert!(page3.has_previous);

        // Verify no overlapping items between pages
        let page1_ids: Vec<_> = page1.items.iter().map(|a| &a.id).collect();
        let page2_ids: Vec<_> = page2.items.iter().map(|a| &a.id).collect();
        let page3_ids: Vec<_> = page3.items.iter().map(|a| &a.id).collect();

        // No IDs should overlap
        for id in &page1_ids {
            assert!(!page2_ids.contains(id));
            assert!(!page3_ids.contains(id));
        }
        for id in &page2_ids {
            assert!(!page3_ids.contains(id));
        }
    }

    #[tokio::test]
    async fn test_unified_transactions_listing() {
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

        // Create multiple transactions
        for i in 1..=15 {
            let transaction = crate::ledger::transaction::patterns::create_sales_transaction(
                format!("txn_{}", i),
                chrono::NaiveDate::from_ymd_opt(2024, 1, i).unwrap(),
                format!("Transaction {}", i),
                cash_account.id.clone(),
                revenue_account.id.clone(),
                BigDecimal::from(100 * i),
            )
            .unwrap();

            ledger.record_transaction(transaction).await.unwrap();
        }

        // Test getting all transactions without pagination
        let all_result = ledger
            .get_transactions(None, None, PaginationOption::All)
            .await
            .unwrap();
        assert_eq!(all_result.items().len(), 15);
        assert!(!all_result.is_paginated());

        // Test unified API with pagination
        let pagination_option = PaginationOption::Paginated(PaginationParams::new(1, 5).unwrap());
        let unified_result = ledger
            .get_transactions(None, None, pagination_option)
            .await
            .unwrap();
        assert!(unified_result.is_paginated());
        assert_eq!(unified_result.items().len(), 5);

        // Test pagination with default settings using unified API
        let pagination = PaginationParams::new(1, 5).unwrap();
        let result_response = ledger
            .get_transactions(None, None, PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        let result = result_response.to_paginated_response();

        assert_eq!(result.items.len(), 5);
        assert_eq!(result.total_count, 15);
        assert_eq!(result.page, 1);
        assert_eq!(result.page_size, 5);
        assert_eq!(result.total_pages, 3);
        assert!(result.has_next);
        assert!(!result.has_previous);

        // Verify transactions are sorted by date descending
        let dates: Vec<_> = result.items.iter().map(|t| t.date).collect();
        for i in 1..dates.len() {
            assert!(dates[i - 1] >= dates[i]);
        }

        // Test account-specific transactions pagination
        let pagination = PaginationParams::new(1, 10).unwrap();
        let account_result_response = ledger
            .get_account_transactions(
                &cash_account.id,
                None,
                None,
                PaginationOption::Paginated(pagination),
            )
            .await
            .unwrap();
        let account_result = account_result_response.to_paginated_response();

        assert_eq!(account_result.items.len(), 10);
        assert_eq!(account_result.total_count, 15);

        // All transactions should affect the cash account
        for txn in &account_result.items {
            assert!(txn.entries.iter().any(|e| e.account_id == cash_account.id));
        }
    }

    #[tokio::test]
    async fn test_pagination_parameters_validation() {
        // Test invalid page number
        let result = PaginationParams::new(0, 10);
        assert!(result.is_err());

        // Test invalid page size (too small)
        let result = PaginationParams::new(1, 0);
        assert!(result.is_err());

        // Test invalid page size (too large)
        let result = PaginationParams::new(1, 1001);
        assert!(result.is_err());

        // Test valid parameters
        let result = PaginationParams::new(1, 50);
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.offset(), 0);
        assert_eq!(params.limit(), 50);

        // Test offset calculation
        let params = PaginationParams::new(3, 20).unwrap();
        assert_eq!(params.offset(), 40); // (3-1) * 20
        assert_eq!(params.limit(), 20);
    }

    #[tokio::test]
    async fn test_unified_api_optimization() {
        let storage = MemoryStorage::new();
        let mut ledger = Ledger::new(storage);

        // Create test accounts
        for i in 1..=10 {
            ledger
                .create_account(
                    format!("acc_{}", i),
                    format!("Account {}", i),
                    AccountType::Asset,
                    None,
                )
                .await
                .unwrap();
        }

        // Test that single unified method can handle both cases

        // Case 1: Get all items (no pagination)
        let all_accounts = ledger.list_accounts(PaginationOption::All).await.unwrap();
        assert!(!all_accounts.is_paginated());
        assert_eq!(all_accounts.items().len(), 10);

        // Case 2: Get paginated results
        let pagination = PaginationParams::new(1, 5).unwrap();
        let paginated_accounts = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
            .unwrap();
        assert!(paginated_accounts.is_paginated());
        assert_eq!(paginated_accounts.items().len(), 5);

        // Test conversion to PaginatedResponse for API compatibility
        let paginated_response = all_accounts.to_paginated_response();
        assert_eq!(paginated_response.items.len(), 10);
        assert_eq!(paginated_response.total_count, 10);
        assert_eq!(paginated_response.page, 1);
        assert_eq!(paginated_response.total_pages, 1);

        // Test convenience methods still work
        let convenience_all = ledger.list_all_accounts().await.unwrap();
        assert_eq!(convenience_all.len(), 10);
    }

    #[tokio::test]
    async fn test_paginated_response_metadata() {
        // Test with exact division
        let response = PaginatedResponse::new(vec![1, 2, 3], 2, 3, 9);
        assert_eq!(response.total_pages, 3);
        assert!(response.has_previous);
        assert!(response.has_next);

        // Test first page
        let response = PaginatedResponse::new(vec![1, 2, 3], 1, 3, 9);
        assert!(!response.has_previous);
        assert!(response.has_next);

        // Test last page
        let response = PaginatedResponse::new(vec![7, 8, 9], 3, 3, 9);
        assert!(response.has_previous);
        assert!(!response.has_next);

        // Test single page
        let response = PaginatedResponse::new(vec![1, 2], 1, 5, 2);
        assert_eq!(response.total_pages, 1);
        assert!(!response.has_previous);
        assert!(!response.has_next);

        // Test empty result
        let response: PaginatedResponse<i32> = PaginatedResponse::new(vec![], 1, 10, 0);
        assert_eq!(response.total_pages, 1);
        assert!(!response.has_previous);
        assert!(!response.has_next);
    }
}
