use crate::ledger::balances::*;
use crate::types::{Account, AccountType, Entry, Transaction};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
}

fn sale(id: &str, date: NaiveDate, amount: i32) -> Transaction {
    let mut txn = Transaction::new(id, date, "Sale", None);
    txn.add_entry(Entry::debit("cash", BigDecimal::from(amount), None));
    txn.add_entry(Entry::credit("sales", BigDecimal::from(amount), None));
    txn
}

#[test]
fn test_account_balance_uses_the_normal_side() {
    let txns = [sale("t1", day(1), 100), sale("t2", day(2), 50)];
    let cash = Account::new("cash", "Cash", AccountType::Asset, None);
    let sales = Account::new("sales", "Sales", AccountType::Income, None);

    assert_eq!(account_balance(&cash, &txns), BigDecimal::from(150));
    assert_eq!(account_balance(&sales, &txns), BigDecimal::from(150));
}

#[test]
fn test_trial_balance_ignores_later_transactions() {
    let txns = [sale("t1", day(1), 100), sale("t2", day(5), 50)];
    let accounts = [
        Account::new("cash", "Cash", AccountType::Asset, None),
        Account::new("sales", "Sales", AccountType::Income, None),
        Account::new("idle", "Idle", AccountType::Expense, None),
    ];

    let tb = trial_balance(day(3), accounts, &txns);

    assert!(tb.is_balanced);
    assert_eq!(tb.total_debits, BigDecimal::from(100));
    assert_eq!(tb.total_credits, BigDecimal::from(100));
    assert_eq!(tb.balances["idle"].balance_amount(), BigDecimal::from(0));
}
