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

/// Sum of the balance amounts in a section
#[must_use]
pub fn total(balances: &[AccountBalance]) -> BigDecimal {
    balances.iter().map(AccountBalance::balance_amount).sum()
}

fn take(by_type: &mut BalancesByType, account_type: AccountType) -> Vec<AccountBalance> {
    by_type.remove(&account_type).unwrap_or_default()
}

/// Balance sheet, with net income from income and expense accounts shown as an equity line
#[must_use]
pub fn balance_sheet(as_of_date: NaiveDate, mut by_type: BalancesByType) -> BalanceSheet {
    let assets = take(&mut by_type, AccountType::Asset);
    let liabilities = take(&mut by_type, AccountType::Liability);
    let mut equity = take(&mut by_type, AccountType::Equity);

    let net_income = total(&take(&mut by_type, AccountType::Income))
        - total(&take(&mut by_type, AccountType::Expense));
    if !net_income.is_zero() {
        let account = Account::new("net_income", "Net Income", AccountType::Equity, None);
        equity.push(AccountBalance::from_balance(account, &net_income));
    }

    let total_assets = total(&assets);
    let total_liabilities = total(&liabilities);
    let total_equity = total(&equity);

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
    let revenue = take(&mut by_type, AccountType::Income);
    let expenses = take(&mut by_type, AccountType::Expense);
    let total_revenue = total(&revenue);
    let total_expenses = total(&expenses);

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

/// Classify a transaction by keywords in its account ids and description
///
/// This is a heuristic: financing if any account id mentions `payable`, `loan`, `equity` or
/// `capital`; investing if an asset or cash account is involved and the description mentions
/// equipment; operating otherwise.
#[must_use]
pub fn classify_cash_flow(transaction: &Transaction) -> CashFlowActivity {
    let mentions = |words: &[&str]| {
        transaction
            .entries
            .iter()
            .any(|e| words.iter().any(|word| e.account_id.contains(word)))
    };

    if mentions(&["payable", "loan", "equity", "capital"]) {
        CashFlowActivity::Financing
    } else if mentions(&["asset", "cash"])
        && transaction.description.to_lowercase().contains("equipment")
    {
        CashFlowActivity::Investing
    } else {
        CashFlowActivity::Operating
    }
}

/// Simplified cash flow statement: each transaction's total debits, in its classified section
#[must_use]
pub fn cash_flow(
    start_date: NaiveDate,
    end_date: NaiveDate,
    transactions: &[Transaction],
) -> CashFlowStatement {
    let section = |activity: CashFlowActivity| -> Vec<CashFlowItem> {
        transactions
            .iter()
            .filter(|t| classify_cash_flow(t) == activity)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()
    }

    fn txn(debit: &str, credit: &str, description: &str) -> Transaction {
        let mut t = Transaction::new("t", day(), description, None);
        t.add_entry(Entry::debit(debit, BigDecimal::from(10), None));
        t.add_entry(Entry::credit(credit, BigDecimal::from(10), None));
        t
    }

    #[test]
    fn test_cash_flow_classification() {
        assert_eq!(
            classify_cash_flow(&txn("cash", "loan_payable", "Loan")),
            CashFlowActivity::Financing
        );
        assert_eq!(
            classify_cash_flow(&txn("fixed_asset", "bank", "Office equipment")),
            CashFlowActivity::Investing
        );
        assert_eq!(
            classify_cash_flow(&txn("cash", "sales", "Sale")),
            CashFlowActivity::Operating
        );
    }

    #[test]
    fn test_balance_sheet_shows_a_loss_as_a_debit_equity_line() {
        let expense = Account::new("rent", "Rent", AccountType::Expense, None);
        let by_type = crate::ledger::balances::group_by_type([AccountBalance::from_balance(
            expense,
            &BigDecimal::from(40),
        )]);

        let sheet = balance_sheet(day(), by_type);
        assert_eq!(sheet.equity.len(), 1);
        assert_eq!(sheet.equity[0].debit_balance, Some(BigDecimal::from(40)));
    }
}
