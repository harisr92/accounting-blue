use crate::reports::*;
use crate::types::{Account, AccountBalance, AccountType, Entry, Transaction};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

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
fn test_net_loss_still_balances() {
    // Owner invests 100 in cash, then 40 of rent is paid from it
    let balance = |id: &str, account_type, amount: i32| {
        AccountBalance::from_balance(
            Account::new(id, id, account_type, None),
            &BigDecimal::from(amount),
        )
    };
    let by_type = crate::ledger::balances::group_by_type([
        balance("cash", AccountType::Asset, 60),
        balance("capital", AccountType::Equity, 100),
        balance("rent", AccountType::Expense, 40),
    ]);

    let sheet = balance_sheet(day(), by_type.clone());
    assert_eq!(sheet.total_assets, BigDecimal::from(60));
    assert_eq!(sheet.total_equity, BigDecimal::from(60));
    assert!(sheet.is_balanced);

    let statement = income_statement(day(), day(), by_type);
    assert_eq!(statement.net_income, BigDecimal::from(-40));
}

#[test]
fn test_overdrawn_asset_reduces_total_assets() {
    let cash = AccountBalance::from_balance(
        Account::new("cash", "Cash", AccountType::Asset, None),
        &BigDecimal::from(-25),
    );
    let bank = AccountBalance::from_balance(
        Account::new("bank", "Bank", AccountType::Asset, None),
        &BigDecimal::from(100),
    );
    assert_eq!(
        total(AccountType::Asset, &[cash, bank]),
        BigDecimal::from(75)
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
