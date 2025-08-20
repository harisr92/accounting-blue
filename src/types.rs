//! Core types and data structures for the accounting system

use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Account types following standard accounting principles
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountType {
    /// Assets - what the business owns (Cash, Inventory, Equipment, etc.)
    Asset,
    /// Liabilities - what the business owes (Loans, Accounts Payable, etc.)
    Liability,
    /// Equity - owner's interest in the business (Capital, Retained Earnings, etc.)
    Equity,
    /// Income/Revenue - money earned by the business
    Income,
    /// Expenses - costs incurred by the business
    Expense,
}

impl AccountType {
    /// Returns the normal balance type for this account type
    /// Assets and Expenses normally have debit balances
    /// Liabilities, Equity, and Income normally have credit balances
    pub fn normal_balance(&self) -> EntryType {
        match self {
            AccountType::Asset | AccountType::Expense => EntryType::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Income => EntryType::Credit,
        }
    }
}

/// Types of entries in double-entry bookkeeping
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryType {
    /// Debit entry - increases Assets and Expenses, decreases Liabilities, Equity, and Income
    Debit,
    /// Credit entry - increases Liabilities, Equity, and Income, decreases Assets and Expenses
    Credit,
}

/// Core account structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Unique identifier for the account
    pub id: String,
    /// Human-readable account name
    pub name: String,
    /// Type of account (Asset, Liability, etc.)
    pub account_type: AccountType,
    /// Optional parent account for hierarchical chart of accounts
    pub parent_id: Option<String>,
    /// Current balance of the account
    pub balance: BigDecimal,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// When the account was created
    pub created_at: NaiveDateTime,
    /// When the account was last updated
    pub updated_at: NaiveDateTime,
}

impl Account {
    /// Create a new account
    pub fn new(
        id: String,
        name: String,
        account_type: AccountType,
        parent_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id,
            name,
            account_type,
            parent_id,
            balance: BigDecimal::from(0),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the account balance based on an entry
    pub fn apply_entry(&mut self, entry_type: EntryType, amount: &BigDecimal) {
        match (self.account_type.normal_balance(), entry_type) {
            // Normal balance side increases
            (EntryType::Debit, EntryType::Debit) | (EntryType::Credit, EntryType::Credit) => {
                self.balance += amount;
            }
            // Opposite side decreases
            (EntryType::Debit, EntryType::Credit) | (EntryType::Credit, EntryType::Debit) => {
                self.balance -= amount;
            }
        }
        self.updated_at = chrono::Utc::now().naive_utc();
    }
}

/// Individual entry within a transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Account being affected
    pub account_id: String,
    /// Type of entry (Debit or Credit)
    pub entry_type: EntryType,
    /// Amount of the entry
    pub amount: BigDecimal,
    /// Optional description for this specific entry
    pub description: Option<String>,
}

impl Entry {
    /// Create a new entry
    pub fn new(
        account_id: String,
        entry_type: EntryType,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        Self {
            account_id,
            entry_type,
            amount,
            description,
        }
    }

    /// Create a debit entry
    pub fn debit(account_id: String, amount: BigDecimal, description: Option<String>) -> Self {
        Self::new(account_id, EntryType::Debit, amount, description)
    }

    /// Create a credit entry
    pub fn credit(account_id: String, amount: BigDecimal, description: Option<String>) -> Self {
        Self::new(account_id, EntryType::Credit, amount, description)
    }
}

/// Complete transaction with multiple entries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier for the transaction
    pub id: String,
    /// Date when the transaction occurred
    pub date: NaiveDate,
    /// List of entries that make up this transaction
    pub entries: Vec<Entry>,
    /// Description of the transaction
    pub description: String,
    /// Optional reference number (invoice number, check number, etc.)
    pub reference: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// When the transaction was created
    pub created_at: NaiveDateTime,
    /// When the transaction was last updated
    pub updated_at: NaiveDateTime,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(
        id: String,
        date: NaiveDate,
        description: String,
        reference: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id,
            date,
            entries: Vec::new(),
            description,
            reference,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add an entry to the transaction
    pub fn add_entry(&mut self, entry: Entry) {
        self.entries.push(entry);
        self.updated_at = chrono::Utc::now().naive_utc();
    }

    /// Calculate total debits
    pub fn total_debits(&self) -> BigDecimal {
        self.entries
            .iter()
            .filter(|e| e.entry_type == EntryType::Debit)
            .map(|e| &e.amount)
            .sum()
    }

    /// Calculate total credits
    pub fn total_credits(&self) -> BigDecimal {
        self.entries
            .iter()
            .filter(|e| e.entry_type == EntryType::Credit)
            .map(|e| &e.amount)
            .sum()
    }

    /// Check if the transaction is balanced (debits = credits)
    pub fn is_balanced(&self) -> bool {
        self.total_debits() == self.total_credits()
    }

    /// Validate the transaction
    pub fn validate(&self) -> Result<(), LedgerError> {
        if self.entries.is_empty() {
            return Err(LedgerError::InvalidTransaction(
                "Transaction must have at least one entry".to_string(),
            ));
        }

        if self.entries.len() < 2 {
            return Err(LedgerError::InvalidTransaction(
                "Transaction must have at least two entries for double-entry bookkeeping"
                    .to_string(),
            ));
        }

        if !self.is_balanced() {
            return Err(LedgerError::InvalidTransaction(format!(
                "Transaction is not balanced: debits = {}, credits = {}",
                self.total_debits(),
                self.total_credits()
            )));
        }

        // Check for zero or negative amounts
        for entry in &self.entries {
            if entry.amount <= BigDecimal::from(0) {
                return Err(LedgerError::InvalidTransaction(
                    "Entry amounts must be positive".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Trial Balance - snapshot of all account balances at a point in time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialBalance {
    /// Date of the trial balance
    pub as_of_date: NaiveDate,
    /// Account balances grouped by type
    pub balances: HashMap<String, AccountBalance>,
    /// Total debits across all accounts
    pub total_debits: BigDecimal,
    /// Total credits across all accounts
    pub total_credits: BigDecimal,
    /// Whether the trial balance is balanced
    pub is_balanced: bool,
}

/// Account balance information for trial balance
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountBalance {
    /// Account information
    pub account: Account,
    /// Debit balance (if applicable)
    pub debit_balance: Option<BigDecimal>,
    /// Credit balance (if applicable)
    pub credit_balance: Option<BigDecimal>,
}

impl AccountBalance {
    /// Get the balance amount regardless of debit/credit
    pub fn balance_amount(&self) -> BigDecimal {
        self.debit_balance
            .clone()
            .or_else(|| self.credit_balance.clone())
            .unwrap_or_else(|| BigDecimal::from(0))
    }
}

/// Errors that can occur in the ledger system
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

/// Result type for ledger operations
pub type LedgerResult<T> = Result<T, LedgerError>;
