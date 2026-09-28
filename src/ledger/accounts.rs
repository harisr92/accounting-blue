//! Account operations: creation, updates and hierarchy lookups against an [`AccountStore`]

use std::collections::{HashMap, HashSet};

use crate::error::{LedgerError, LedgerResult};
use crate::traits::{AccountStore, AccountValidator};
use crate::types::{Account, AccountType, PaginationOption};

/// A standard chart of accounts for a small business: `(key, id, name, type)`
///
/// [`Ledger::setup_standard_chart_of_accounts`](crate::Ledger::setup_standard_chart_of_accounts)
/// creates these accounts and returns them keyed by the first column.
pub const STANDARD_CHART: [(&str, &str, &str, AccountType); 12] = [
    ("cash", "1000", "Cash", AccountType::Asset),
    (
        "accounts_receivable",
        "1200",
        "Accounts Receivable",
        AccountType::Asset,
    ),
    ("inventory", "1300", "Inventory", AccountType::Asset),
    (
        "accounts_payable",
        "2000",
        "Accounts Payable",
        AccountType::Liability,
    ),
    (
        "loans_payable",
        "2100",
        "Loans Payable",
        AccountType::Liability,
    ),
    (
        "owners_equity",
        "3000",
        "Owner's Equity",
        AccountType::Equity,
    ),
    (
        "retained_earnings",
        "3200",
        "Retained Earnings",
        AccountType::Equity,
    ),
    (
        "sales_revenue",
        "4000",
        "Sales Revenue",
        AccountType::Income,
    ),
    (
        "service_revenue",
        "4100",
        "Service Revenue",
        AccountType::Income,
    ),
    (
        "cost_of_goods_sold",
        "5000",
        "Cost of Goods Sold",
        AccountType::Expense,
    ),
    ("rent_expense", "6000", "Rent Expense", AccountType::Expense),
    (
        "utilities_expense",
        "6100",
        "Utilities Expense",
        AccountType::Expense,
    ),
];

/// Load an account, failing when it does not exist
pub(crate) async fn require_account<S: AccountStore>(
    store: &S,
    account_id: &str,
) -> LedgerResult<Account> {
    store
        .get_account(account_id)
        .await?
        .ok_or_else(|| LedgerError::AccountNotFound(account_id.to_string()))
}

/// Validate and save a new account whose id is free and whose parent, if any, exists
pub(crate) async fn create_account<S: AccountStore>(
    store: &mut S,
    validator: &dyn AccountValidator,
    account: Account,
) -> LedgerResult<Account> {
    validator.validate_account(&account)?;

    if store.get_account(&account.id).await?.is_some() {
        return Err(LedgerError::DuplicateAccount(account.id));
    }

    if let Some(parent_id) = &account.parent_id {
        if store.get_account(parent_id).await?.is_none() {
            return Err(LedgerError::ParentNotFound(parent_id.clone()));
        }
    }

    store.save_account(&account).await?;
    Ok(account)
}

/// Validate and replace an existing account
pub(crate) async fn update_account<S: AccountStore>(
    store: &mut S,
    validator: &dyn AccountValidator,
    account: &Account,
) -> LedgerResult<()> {
    validator.validate_account(account)?;
    store.update_account(account).await
}

/// Create every account in [`STANDARD_CHART`]
pub(crate) async fn create_standard_chart<S: AccountStore>(
    store: &mut S,
    validator: &dyn AccountValidator,
) -> LedgerResult<HashMap<String, Account>> {
    let mut accounts = HashMap::with_capacity(STANDARD_CHART.len());
    for (key, id, name, account_type) in STANDARD_CHART {
        let account = Account::new(id, name, account_type, None);
        accounts.insert(
            key.to_string(),
            create_account(store, validator, account).await?,
        );
    }
    Ok(accounts)
}

/// Direct children of an account
pub(crate) async fn child_accounts<S: AccountStore>(
    store: &S,
    parent_id: &str,
) -> LedgerResult<Vec<Account>> {
    let accounts = store.list_accounts(None, PaginationOption::All).await?;
    Ok(accounts
        .into_items()
        .into_iter()
        .filter(|account| account.parent_id.as_deref() == Some(parent_id))
        .collect())
}

/// The chain of accounts from the root down to `account_id`
///
/// Fails with [`LedgerError::AccountCycle`] if the parent links loop back on themselves.
pub(crate) async fn account_path<S: AccountStore>(
    store: &S,
    account_id: &str,
) -> LedgerResult<Vec<Account>> {
    let mut path = Vec::new();
    let mut visited = HashSet::new();
    let mut next = Some(account_id.to_string());

    while let Some(id) = next {
        if !visited.insert(id.clone()) {
            return Err(LedgerError::AccountCycle(id));
        }
        let account = require_account(store, &id).await?;
        let parent = account.parent_id.clone();
        path.push(account);
        next = parent;
    }

    path.reverse();
    Ok(path)
}
