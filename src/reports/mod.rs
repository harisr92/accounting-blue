//! Financial reports as pure functions of balances and transactions
//!
//! [`Ledger`](crate::Ledger) loads the data and calls these; they can equally be used over data
//! loaded some other way.

pub mod types;

use bigdecimal::{BigDecimal, Zero};
use chrono::NaiveDate;
use std::collections::HashMap;

use crate::types::{Account, AccountBalance, AccountType, Transaction, TrialBalance};

pub use types::{
    BalanceSheet, CashFlowItem, CashFlowStatement, IncomeStatement, LedgerIntegrityReport,
};

/// Account balances grouped by type, as produced by [`crate::ledger::balances::group_by_type`]
pub type BalancesByType = HashMap<AccountType, Vec<AccountBalance>>;

/// Total of a section of one account type, measured on that type's normal side
///
/// A balance on the opposite side (an overdrawn asset, a debit equity line for a net loss)
/// reduces the total rather than adding to it.
#[must_use]
pub fn total(account_type: AccountType, balances: &[AccountBalance]) -> BigDecimal {
    let side = account_type.normal_balance();
    balances.iter().map(|balance| balance.net_on(side)).sum()
}

fn take(by_type: &mut BalancesByType, account_type: AccountType) -> Vec<AccountBalance> {
    by_type.remove(&account_type).unwrap_or_default()
}

/// Section of one account type together with its total
fn section(
    by_type: &mut BalancesByType,
    account_type: AccountType,
) -> (Vec<AccountBalance>, BigDecimal) {
    let balances = take(by_type, account_type);
    let sum = total(account_type, &balances);
    (balances, sum)
}

/// Balance sheet, with net income from income and expense accounts shown as an equity line
#[must_use]
pub fn balance_sheet(as_of_date: NaiveDate, mut by_type: BalancesByType) -> BalanceSheet {
    let (assets, total_assets) = section(&mut by_type, AccountType::Asset);
    let (liabilities, total_liabilities) = section(&mut by_type, AccountType::Liability);
    let mut equity = take(&mut by_type, AccountType::Equity);

    let (_, income) = section(&mut by_type, AccountType::Income);
    let (_, expenses) = section(&mut by_type, AccountType::Expense);
    let net_income = income - expenses;
    if !net_income.is_zero() {
        let account = Account::new("net_income", "Net Income", AccountType::Equity, None);
        equity.push(AccountBalance::from_balance(account, &net_income));
    }
    let total_equity = total(AccountType::Equity, &equity);

    BalanceSheet {
        as_of_date,
        is_balanced: total_assets == &total_liabilities + &total_equity,
        assets,
        liabilities,
        equity,
        total_assets,
        total_liabilities,
        total_equity,
    }
}

/// Income statement from income and expense balances
///
/// The balances are cumulative up to `end_date`; `start_date` only labels the period.
#[must_use]
pub fn income_statement(
    start_date: NaiveDate,
    end_date: NaiveDate,
    mut by_type: BalancesByType,
) -> IncomeStatement {
    let (revenue, total_revenue) = section(&mut by_type, AccountType::Income);
    let (expenses, total_expenses) = section(&mut by_type, AccountType::Expense);

    IncomeStatement {
        start_date,
        end_date,
        net_income: &total_revenue - &total_expenses,
        revenue,
        expenses,
        total_revenue,
        total_expenses,
    }
}

/// Cash flow statement section a transaction belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashFlowActivity {
    /// Day-to-day business
    Operating,
    /// Purchases of long-lived assets
    Investing,
    /// Loans and owner's equity
    Financing,
}

/// Words in a liability's id or name that mark it as borrowing or an amount owed to owners, a
/// financing activity
const BORROWING_WORDS: [&str; 9] = [
    "loan",
    "borrow",
    "debt",
    "debenture",
    "mortgage",
    "overdraft",
    "note payable",
    "notes payable",
    "dividend",
];

/// Words in an asset's id or name that mark it as long-lived, so buying or selling it is investing
const FIXED_ASSET_WORDS: [&str; 8] = [
    "fixed asset",
    "equipment",
    "machinery",
    "plant",
    "furniture",
    "vehicle",
    "building",
    "property",
];

/// Words in an asset's id or name that mark it as a contra account against a long-lived asset
const CONTRA_ASSET_WORDS: [&str; 4] =
    ["depreciation", "amortisation", "amortization", "impairment"];

/// Whether any of `words` appears in `text`, ignoring case and treating `_` and `-` as spaces
fn mentions_any(text: &str, words: &[&str]) -> bool {
    let text = text.to_lowercase().replace(['_', '-'], " ");
    words.iter().any(|word| text.contains(word))
}

/// Whether any of `words` appears in the account's id or name, ignoring case
fn account_mentions(account: &Account, words: &[&str]) -> bool {
    mentions_any(&account.id, words) || mentions_any(&account.name, words)
}

/// A contra account against a long-lived asset, such as accumulated depreciation
fn is_contra_asset(account: &Account) -> bool {
    account.account_type == AccountType::Asset && account_mentions(account, &CONTRA_ASSET_WORDS)
}

/// A long-lived asset: property, plant, equipment and the like, but not its contra accounts
fn is_fixed_asset(account: &Account) -> bool {
    account.account_type == AccountType::Asset
        && account_mentions(account, &FIXED_ASSET_WORDS)
        && !is_contra_asset(account)
}

/// What a long-lived asset can be exchanged for: another asset, such as cash, or a liability
fn is_consideration(account: &Account) -> bool {
    match account.account_type {
        AccountType::Asset => !is_fixed_asset(account) && !is_contra_asset(account),
        AccountType::Liability => true,
        _ => false,
    }
}

/// Classify a transaction by the accounts it posts to
///
/// `accounts` is keyed by account id; an entry whose account is missing from it counts as
/// operating. This is a heuristic:
///
/// - **Financing** if an entry is on a liability account whose id or name mentions a loan,
///   borrowing, debt, debentures, a mortgage, an overdraft, notes payable or dividends, or on an
///   equity account alongside an asset account (owner contributions and withdrawals). A closing
///   entry between income or expense accounts and retained earnings moves no cash, so it is not
///   financing.
/// - **Investing** if an entry is on a long-lived asset account (its id or name mentions a fixed
///   asset, equipment, machinery, plant, furniture, a vehicle, a building or property) and another
///   is on what it is exchanged for: a non-fixed asset such as cash, or a liability for a
///   purchase on credit. Accumulated depreciation, amortisation and impairment accounts are not
///   long-lived assets, and the description is not read, so depreciation, repairs and write-downs
///   are not investing.
/// - **Operating** otherwise, including purchases on trade payables.
///
/// Keywords match case-insensitively with `_` and `-` read as spaces, so both descriptive ids
/// (`loan_payable`, `fixed_asset`) and numbered charts such as
/// [`STANDARD_CHART`](crate::STANDARD_CHART) (`2100` "Loans Payable") classify.
#[must_use]
pub fn classify_cash_flow(
    transaction: &Transaction,
    accounts: &HashMap<String, Account>,
) -> CashFlowActivity {
    let posted: Vec<&Account> = transaction
        .entries
        .iter()
        .filter_map(|entry| accounts.get(&entry.account_id))
        .collect();

    let posts_to_asset = posted
        .iter()
        .any(|account| account.account_type == AccountType::Asset);
    let is_financing = |account: &&Account| match account.account_type {
        AccountType::Equity => posts_to_asset,
        AccountType::Liability => account_mentions(account, &BORROWING_WORDS),
        _ => false,
    };
    let is_investing = posted.iter().any(|account| is_fixed_asset(account))
        && posted.iter().any(|account| is_consideration(account));

    if posted.iter().any(is_financing) {
        CashFlowActivity::Financing
    } else if is_investing {
        CashFlowActivity::Investing
    } else {
        CashFlowActivity::Operating
    }
}

/// Simplified cash flow statement: each transaction's total debits, in its classified section
///
/// `accounts` is keyed by account id and is used by [`classify_cash_flow`].
#[must_use]
pub fn cash_flow(
    start_date: NaiveDate,
    end_date: NaiveDate,
    transactions: &[Transaction],
    accounts: &HashMap<String, Account>,
) -> CashFlowStatement {
    let section = |activity: CashFlowActivity| -> Vec<CashFlowItem> {
        transactions
            .iter()
            .filter(|t| classify_cash_flow(t, accounts) == activity)
            .map(|t| CashFlowItem {
                description: t.description.clone(),
                amount: t.total_debits(),
            })
            .collect()
    };
    let sum = |items: &[CashFlowItem]| -> BigDecimal { items.iter().map(|i| &i.amount).sum() };

    let operating_activities = section(CashFlowActivity::Operating);
    let investing_activities = section(CashFlowActivity::Investing);
    let financing_activities = section(CashFlowActivity::Financing);
    let net_operating_cash_flow = sum(&operating_activities);
    let net_investing_cash_flow = sum(&investing_activities);
    let net_financing_cash_flow = sum(&financing_activities);

    CashFlowStatement {
        start_date,
        end_date,
        net_cash_flow: &net_operating_cash_flow
            + &net_investing_cash_flow
            + &net_financing_cash_flow,
        operating_activities,
        investing_activities,
        financing_activities,
        net_operating_cash_flow,
        net_investing_cash_flow,
        net_financing_cash_flow,
    }
}

/// Check that the trial balance and the balance sheet both balance
#[must_use]
pub fn integrity_report(
    as_of_date: NaiveDate,
    trial_balance: TrialBalance,
    balance_sheet: BalanceSheet,
) -> LedgerIntegrityReport {
    let total_liabilities_equity = &balance_sheet.total_liabilities + &balance_sheet.total_equity;

    let issues: Vec<String> = [
        (!trial_balance.is_balanced).then(|| {
            format!(
                "Trial balance is not balanced: debits = {}, credits = {}",
                trial_balance.total_debits, trial_balance.total_credits
            )
        }),
        (!balance_sheet.is_balanced).then(|| {
            format!(
                "Balance sheet is not balanced: assets = {}, liabilities + equity = {}",
                balance_sheet.total_assets, total_liabilities_equity
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect();

    LedgerIntegrityReport {
        as_of_date,
        is_valid: issues.is_empty(),
        issues,
        trial_balance_total_debits: trial_balance.total_debits,
        trial_balance_total_credits: trial_balance.total_credits,
        balance_sheet_total_assets: balance_sheet.total_assets,
        balance_sheet_total_liabilities_equity: total_liabilities_equity,
    }
}
