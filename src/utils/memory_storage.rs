//! In-memory storage implementation for testing

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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

    async fn list_accounts(&self, account_type: Option<AccountType>) -> LedgerResult<Vec<Account>> {
        let accounts = self.accounts.read().unwrap();
        let filtered: Vec<Account> = accounts
            .values()
            .filter(|account| {
                account_type
                    .as_ref()
                    .is_none_or(|t| &account.account_type == t)
            })
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn update_account(&mut self, account: &Account) -> LedgerResult<()> {
        if self.accounts.read().unwrap().contains_key(&account.id) {
            self.accounts
                .write()
                .unwrap()
                .insert(account.id.clone(), account.clone());
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
    ) -> LedgerResult<Vec<Transaction>> {
        let transactions = self.transactions.read().unwrap();
        let filtered: Vec<Transaction> = transactions
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
        Ok(filtered)
    }

    async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        let transactions = self.transactions.read().unwrap();
        let filtered: Vec<Transaction> = transactions
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
        Ok(filtered)
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
            .get_account_transactions(account_id, None, as_of_date)
            .await?;

        for transaction in transactions {
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
        let accounts = self.list_accounts(None).await?;
        let mut balances = HashMap::new();
        let mut total_debits = BigDecimal::from(0);
        let mut total_credits = BigDecimal::from(0);

        for account in accounts {
            let balance = self
                .get_account_balance(&account.id, Some(as_of_date))
                .await?;

            let account_balance = match account.account_type.normal_balance() {
                EntryType::Debit => {
                    if balance >= BigDecimal::from(0) {
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
                    if balance >= BigDecimal::from(0) {
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
            let account_type = account_balance.account.account_type.clone();
            result
                .entry(account_type)
                .or_default()
                .push(account_balance);
        }

        Ok(result)
    }
}
