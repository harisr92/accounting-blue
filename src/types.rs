//! Core types and data structures for the accounting system

use bigdecimal::{BigDecimal, Signed};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{LedgerError, LedgerResult, PaginationError};

/// Account types following standard accounting principles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[must_use]
    pub fn normal_balance(self) -> EntryType {
        match self {
            AccountType::Asset | AccountType::Expense => EntryType::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Income => EntryType::Credit,
        }
    }

    /// How an entry moves the balance of an account of this type: an entry on the normal-balance
    /// side increases it, an entry on the opposite side decreases it
    #[must_use]
    pub fn balance_effect(self, entry_type: EntryType, amount: &BigDecimal) -> BigDecimal {
        if entry_type == self.normal_balance() {
            amount.clone()
        } else {
            -amount
        }
    }
}

/// Types of entries in double-entry bookkeeping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryType {
    /// Debit entry - increases Assets and Expenses, decreases Liabilities, Equity, and Income
    Debit,
    /// Credit entry - increases Liabilities, Equity, and Income, decreases Assets and Expenses
    Credit,
}

impl EntryType {
    /// The other side of the ledger
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Debit => Self::Credit,
            Self::Credit => Self::Debit,
        }
    }

    /// `amount` signed debit-positive: a debit keeps its sign, a credit is negated
    #[must_use]
    pub fn signed(self, amount: &BigDecimal) -> BigDecimal {
        match self {
            Self::Debit => amount.clone(),
            Self::Credit => -amount,
        }
    }
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
    /// Create a new account with a zero balance
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        account_type: AccountType,
        parent_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: id.into(),
            name: name.into(),
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
        self.balance += self.account_type.balance_effect(entry_type, amount);
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
        account_id: impl Into<String>,
        entry_type: EntryType,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            entry_type,
            amount,
            description,
        }
    }

    /// Create a debit entry
    pub fn debit(
        account_id: impl Into<String>,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        Self::new(account_id, EntryType::Debit, amount, description)
    }

    /// Create a credit entry
    pub fn credit(
        account_id: impl Into<String>,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        Self::new(account_id, EntryType::Credit, amount, description)
    }

    /// The same entry on the opposite side, which undoes its effect on a balance
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            entry_type: self.entry_type.opposite(),
            ..self.clone()
        }
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
    /// Create a new transaction with no entries
    pub fn new(
        id: impl Into<String>,
        date: NaiveDate,
        description: impl Into<String>,
        reference: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: id.into(),
            date,
            entries: Vec::new(),
            description: description.into(),
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
    #[must_use]
    pub fn total_debits(&self) -> BigDecimal {
        self.total_of(EntryType::Debit)
    }

    /// Calculate total credits
    #[must_use]
    pub fn total_credits(&self) -> BigDecimal {
        self.total_of(EntryType::Credit)
    }

    fn total_of(&self, entry_type: EntryType) -> BigDecimal {
        self.entries
            .iter()
            .filter(|e| e.entry_type == entry_type)
            .map(|e| &e.amount)
            .sum()
    }

    /// Check if the transaction is balanced (debits = credits)
    #[must_use]
    pub fn is_balanced(&self) -> bool {
        self.total_debits() == self.total_credits()
    }

    /// Whether any entry posts to `account_id`
    #[must_use]
    pub fn involves(&self, account_id: &str) -> bool {
        self.entries.iter().any(|e| e.account_id == account_id)
    }

    /// Validate the double-entry rules: at least two entries, balanced, and all amounts positive
    ///
    /// # Errors
    ///
    /// [`LedgerError::TooFewEntries`], [`LedgerError::Unbalanced`] or
    /// [`LedgerError::NonPositiveAmount`], checked in that order.
    pub fn validate(&self) -> LedgerResult<()> {
        if self.entries.len() < 2 {
            return Err(LedgerError::TooFewEntries);
        }

        let (debits, credits) = (self.total_debits(), self.total_credits());
        if debits != credits {
            return Err(LedgerError::Unbalanced { debits, credits });
        }

        if self.entries.iter().any(|e| !e.amount.is_positive()) {
            return Err(LedgerError::NonPositiveAmount);
        }

        Ok(())
    }
}

/// Which transactions a listing should return
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionFilter {
    /// Only transactions with an entry for this account
    pub account_id: Option<String>,
    /// Only transactions dated on or after this day
    pub start_date: Option<NaiveDate>,
    /// Only transactions dated on or before this day
    pub end_date: Option<NaiveDate>,
}

impl TransactionFilter {
    /// Transactions dated within an inclusive, optionally open-ended range
    #[must_use]
    pub fn between(start_date: Option<NaiveDate>, end_date: Option<NaiveDate>) -> Self {
        Self {
            account_id: None,
            start_date,
            end_date,
        }
    }

    /// Restrict the filter to transactions that post to `account_id`
    #[must_use]
    pub fn for_account(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = Some(account_id.into());
        self
    }

    /// Whether `date` falls within the range
    #[must_use]
    pub fn contains_date(&self, date: NaiveDate) -> bool {
        self.start_date.is_none_or(|start| date >= start)
            && self.end_date.is_none_or(|end| date <= end)
    }

    /// Whether a transaction passes the filter
    #[must_use]
    pub fn matches(&self, transaction: &Transaction) -> bool {
        self.contains_date(transaction.date)
            && self
                .account_id
                .as_deref()
                .is_none_or(|id| transaction.involves(id))
    }
}

/// Trial Balance - snapshot of all account balances at a point in time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialBalance {
    /// Date of the trial balance
    pub as_of_date: NaiveDate,
    /// Account balances keyed by account id
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
    /// Place a balance on the debit or credit column
    ///
    /// `balance` is measured the way [`Account::balance`] is: positive on the account's normal
    /// side. A non-negative balance sits on the normal side; a negative one is shown, as a positive
    /// figure, on the opposite side.
    #[must_use]
    pub fn from_balance(account: Account, balance: &BigDecimal) -> Self {
        let normal = account.account_type.normal_balance();
        let side = if balance.is_negative() {
            normal.opposite()
        } else {
            normal
        };
        let amount = Some(balance.abs());

        match side {
            EntryType::Debit => Self {
                account,
                debit_balance: amount,
                credit_balance: None,
            },
            EntryType::Credit => Self {
                account,
                debit_balance: None,
                credit_balance: amount,
            },
        }
    }

    /// Get the balance amount regardless of debit/credit
    #[must_use]
    pub fn balance_amount(&self) -> BigDecimal {
        self.debit_balance
            .as_ref()
            .or(self.credit_balance.as_ref())
            .cloned()
            .unwrap_or_default()
    }
}

/// Largest page size [`PaginationParams::new`] accepts
pub const MAX_PAGE_SIZE: u32 = 1000;

/// Pagination options for listing operations
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum PaginationOption {
    /// Return all items without pagination
    #[default]
    All,
    /// Return paginated results
    Paginated(PaginationParams),
}

impl PaginationOption {
    /// Cut an already filtered and sorted list down to the requested page
    #[must_use]
    pub fn paginate<T>(self, items: Vec<T>) -> ListResponse<T> {
        match self {
            Self::All => ListResponse::All(items),
            Self::Paginated(params) => ListResponse::Paginated(params.page_of(items)),
        }
    }
}

impl From<PaginationParams> for PaginationOption {
    fn from(params: PaginationParams) -> Self {
        Self::Paginated(params)
    }
}

/// Pagination parameters for listing operations
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaginationParams {
    /// Page number (starting from 1)
    pub page: u32,
    /// Number of items per page (max 1000)
    pub page_size: u32,
}

impl PaginationParams {
    /// Create new pagination parameters with validation
    ///
    /// # Errors
    ///
    /// [`LedgerError::InvalidPagination`] when `page` is 0 or `page_size` is outside
    /// `1..=`[`MAX_PAGE_SIZE`].
    pub fn new(page: u32, page_size: u32) -> LedgerResult<Self> {
        if page < 1 {
            return Err(PaginationError::PageOutOfRange.into());
        }
        if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
            return Err(PaginationError::PageSizeOutOfRange { max: MAX_PAGE_SIZE }.into());
        }
        Ok(Self { page, page_size })
    }

    /// Get the offset for database queries
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.page.saturating_sub(1).saturating_mul(self.page_size)
    }

    /// Get the limit for database queries
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.page_size
    }

    /// Slice this page out of the full list of items
    #[must_use]
    pub fn page_of<T>(&self, items: Vec<T>) -> PaginatedResponse<T> {
        let total_count = saturating_u32(items.len());
        let page_items = items
            .into_iter()
            .skip(to_usize(self.offset()))
            .take(to_usize(self.limit()))
            .collect();

        PaginatedResponse::new(page_items, self.page, self.page_size, total_count)
    }
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 50,
        }
    }
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Unified response that can contain either all items or paginated results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListResponse<T> {
    /// All items returned without pagination
    All(Vec<T>),
    /// Paginated results with metadata
    Paginated(PaginatedResponse<T>),
}

impl<T> ListResponse<T> {
    /// Get the items regardless of response type
    #[must_use]
    pub fn items(&self) -> &[T] {
        match self {
            Self::All(items) => items,
            Self::Paginated(response) => &response.items,
        }
    }

    /// Get items as owned vector
    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        match self {
            Self::All(items) => items,
            Self::Paginated(response) => response.items,
        }
    }

    /// Check if this is a paginated response
    #[must_use]
    pub fn is_paginated(&self) -> bool {
        matches!(self, Self::Paginated(_))
    }

    /// Get pagination metadata if available
    #[must_use]
    pub fn pagination_info(&self) -> Option<&PaginatedResponse<T>> {
        match self {
            Self::All(_) => None,
            Self::Paginated(response) => Some(response),
        }
    }

    /// Convert to [`PaginatedResponse`], presenting an unpaginated list as a single page
    #[must_use]
    pub fn into_paginated_response(self) -> PaginatedResponse<T> {
        match self {
            Self::All(items) => {
                let count = saturating_u32(items.len());
                PaginatedResponse::new(items, 1, count, count)
            }
            Self::Paginated(response) => response,
        }
    }
}

/// Paginated response containing items and metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// The items for this page
    pub items: Vec<T>,
    /// Current page number
    pub page: u32,
    /// Number of items per page
    pub page_size: u32,
    /// Total number of items across all pages
    pub total_count: u32,
    /// Total number of pages
    pub total_pages: u32,
    /// Whether there is a next page
    pub has_next: bool,
    /// Whether there is a previous page
    pub has_previous: bool,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response
    #[must_use]
    pub fn new(items: Vec<T>, page: u32, page_size: u32, total_count: u32) -> Self {
        let total_pages = if total_count == 0 {
            1
        } else {
            total_count.div_ceil(page_size.max(1))
        };

        Self {
            items,
            page,
            page_size,
            total_count,
            total_pages,
            has_next: page < total_pages,
            has_previous: page > 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_effect_follows_normal_side() {
        let amount = BigDecimal::from(100);
        assert_eq!(
            AccountType::Asset.balance_effect(EntryType::Debit, &amount),
            amount
        );
        assert_eq!(
            AccountType::Asset.balance_effect(EntryType::Credit, &amount),
            -&amount
        );
        assert_eq!(
            AccountType::Income.balance_effect(EntryType::Credit, &amount),
            amount
        );
    }

    #[test]
    fn test_entry_reversed_flips_side_only() {
        let entry = Entry::debit("cash", BigDecimal::from(5), None);
        let reversed = entry.reversed();
        assert_eq!(reversed.entry_type, EntryType::Credit);
        assert_eq!(reversed.amount, entry.amount);
        assert_eq!(reversed.reversed(), entry);
    }

    #[test]
    fn test_from_balance_places_negative_balances_on_the_other_side() {
        let cash = Account::new("cash", "Cash", AccountType::Asset, None);
        let overdrawn = AccountBalance::from_balance(cash.clone(), &BigDecimal::from(-10));
        assert_eq!(overdrawn.credit_balance, Some(BigDecimal::from(10)));
        assert_eq!(overdrawn.debit_balance, None);

        let zero = AccountBalance::from_balance(cash, &BigDecimal::from(0));
        assert_eq!(zero.debit_balance, Some(BigDecimal::from(0)));
    }

    #[test]
    fn test_paginate_past_the_end_is_empty() {
        let params = PaginationParams::new(5, 10).unwrap();
        let page = params.page_of((0..25).collect::<Vec<_>>());
        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 25);
        assert_eq!(page.total_pages, 3);
    }

    #[test]
    fn test_paginated_response_tolerates_zero_page_size() {
        let response = PaginatedResponse::new(vec![1], 1, 0, 1);
        assert_eq!(response.total_pages, 1);
    }

    #[test]
    fn test_transaction_filter() {
        let day = |d| NaiveDate::from_ymd_opt(2024, 1, d).unwrap();
        let mut txn = Transaction::new("t", day(10), "x", None);
        txn.add_entry(Entry::debit("cash", BigDecimal::from(1), None));

        assert!(TransactionFilter::default().matches(&txn));
        assert!(TransactionFilter::between(Some(day(10)), Some(day(10))).matches(&txn));
        assert!(!TransactionFilter::between(Some(day(11)), None).matches(&txn));
        assert!(!TransactionFilter::default()
            .for_account("bank")
            .matches(&txn));
        assert!(TransactionFilter::default()
            .for_account("cash")
            .matches(&txn));
    }
}
