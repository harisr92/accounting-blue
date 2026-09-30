use crate::ledger::STANDARD_CHART;
use crate::reports::*;
use crate::types::{Account, AccountBalance, AccountType, Entry, Transaction};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashMap;

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()
}

fn txn(debit: &str, credit: &str, description: &str) -> Transaction {
    let mut t = Transaction::new("t", day(), description, None);
    t.add_entry(Entry::debit(debit, BigDecimal::from(10), None));
    t.add_entry(Entry::credit(credit, BigDecimal::from(10), None));
    t
}

/// Accounts keyed by id, from `(id, name, type)`
fn chart(accounts: &[(&str, &str, AccountType)]) -> HashMap<String, Account> {
    accounts
        .iter()
        .map(|&(id, name, account_type)| {
            (id.to_string(), Account::new(id, name, account_type, None))
        })
        .collect()
}

/// The standard chart as `setup_standard_chart_of_accounts` creates it, plus a numbered
/// equipment account
fn standard_chart() -> HashMap<String, Account> {
    let mut accounts: Vec<(&str, &str, AccountType)> = STANDARD_CHART
        .iter()
        .map(|&(_, id, name, account_type)| (id, name, account_type))
        .collect();
    accounts.push(("1500", "Office Equipment", AccountType::Asset));
    chart(&accounts)
}

#[test]
fn test_cash_flow_classification() {
    let accounts = chart(&[
        ("cash", "Cash", AccountType::Asset),
        ("bank", "Bank", AccountType::Asset),
        ("fixed_asset", "Fixed Asset", AccountType::Asset),
        ("loan_payable", "Loan Payable", AccountType::Liability),
        ("sales", "Sales", AccountType::Income),
    ]);
    assert_eq!(
        classify_cash_flow(&txn("cash", "loan_payable", "Loan"), &accounts),
        CashFlowActivity::Financing
    );
    assert_eq!(
        classify_cash_flow(&txn("fixed_asset", "bank", "Office equipment"), &accounts),
        CashFlowActivity::Investing
    );
    assert_eq!(
        classify_cash_flow(&txn("cash", "sales", "Sale"), &accounts),
        CashFlowActivity::Operating
    );
}

#[test]
fn test_standard_chart_financing_is_recognised_by_account_type_and_name() {
    let accounts = standard_chart();
    // Owner's Equity (3000) and Loans Payable (2100)
    assert_eq!(
        classify_cash_flow(&txn("1000", "3000", "Owner investment"), &accounts),
        CashFlowActivity::Financing
    );
    assert_eq!(
        classify_cash_flow(&txn("1000", "2100", "Bank loan"), &accounts),
        CashFlowActivity::Financing
    );
    assert_eq!(
        classify_cash_flow(&txn("3200", "1000", "Drawings"), &accounts),
        CashFlowActivity::Financing
    );
}

#[test]
fn test_other_borrowing_and_dividend_liabilities_are_financing() {
    let accounts = chart(&[
        ("cash", "Cash", AccountType::Asset),
        ("2300", "Notes Payable", AccountType::Liability),
        ("2400", "Mortgage Payable", AccountType::Liability),
        ("2500", "Bank Overdraft", AccountType::Liability),
        ("2600", "Dividends Payable", AccountType::Liability),
    ]);
    for transaction in [
        txn("cash", "2300", "Note issued to the bank"),
        txn("2400", "cash", "Mortgage instalment"),
        txn("cash", "2500", "Overdraft drawn"),
        txn("2600", "cash", "Dividend paid"),
    ] {
        assert_eq!(
            classify_cash_flow(&transaction, &accounts),
            CashFlowActivity::Financing,
            "{}",
            transaction.description
        );
    }
}

#[test]
fn test_standard_chart_equipment_purchase_is_investing() {
    let accounts = standard_chart();
    // The asset account decides, whatever the description says
    assert_eq!(
        classify_cash_flow(&txn("1500", "1000", "Bought equipment"), &accounts),
        CashFlowActivity::Investing
    );
    assert_eq!(
        classify_cash_flow(&txn("1500", "1000", "Laptop for accounts team"), &accounts),
        CashFlowActivity::Investing
    );
}

/// The standard chart with fixed assets, their accumulated depreciation, and the expense
/// accounts that post against them
fn chart_with_fixed_assets() -> HashMap<String, Account> {
    let mut accounts = standard_chart();
    accounts.extend(chart(&[
        (
            "1510",
            "Accumulated Depreciation - Equipment",
            AccountType::Asset,
        ),
        ("1600", "Delivery Vehicle", AccountType::Asset),
        ("6200", "Depreciation Expense", AccountType::Expense),
        ("6300", "Repairs and Maintenance", AccountType::Expense),
    ]));
    accounts
}

#[test]
fn test_buying_or_selling_a_fixed_asset_is_investing() {
    let accounts = chart_with_fixed_assets();
    for transaction in [
        txn("1600", "1000", "Van"),
        // Bought on credit from the supplier
        txn("1500", "2000", "Printer on 30-day terms"),
        // Sold for cash
        txn("1000", "1600", "Old van sold"),
    ] {
        assert_eq!(
            classify_cash_flow(&transaction, &accounts),
            CashFlowActivity::Investing,
            "{}",
            transaction.description
        );
    }
}

#[test]
fn test_depreciation_repairs_and_write_downs_are_not_investing() {
    let accounts = chart_with_fixed_assets();
    for transaction in [
        txn("6200", "1510", "Monthly depreciation on equipment"),
        txn("6300", "1000", "Equipment repair"),
        // Written down straight against the asset, with nothing received for it
        txn("6200", "1600", "Van written down"),
        // A fully depreciated asset retired against its accumulated depreciation
        txn("1510", "1500", "Old equipment retired"),
    ] {
        assert_eq!(
            classify_cash_flow(&transaction, &accounts),
            CashFlowActivity::Operating,
            "{}",
            transaction.description
        );
    }
}

#[test]
fn test_an_equipment_description_alone_is_not_investing() {
    let accounts = standard_chart();
    // Stock is not a long-lived asset, whatever it is called on the invoice
    assert_eq!(
        classify_cash_flow(&txn("1300", "1000", "Equipment for resale"), &accounts),
        CashFlowActivity::Operating
    );
}

#[test]
fn test_trading_on_the_standard_chart_is_operating() {
    let accounts = standard_chart();
    for transaction in [
        txn("1000", "4000", "Cash sale"),
        txn("6000", "1000", "Office rent"),
        // Trade payables are working capital, not borrowing
        txn("1300", "2000", "Stock bought on credit"),
        txn("2000", "1000", "Supplier paid"),
    ] {
        assert_eq!(
            classify_cash_flow(&transaction, &accounts),
            CashFlowActivity::Operating,
            "{}",
            transaction.description
        );
    }
}

#[test]
fn test_entries_on_unknown_accounts_count_as_operating() {
    assert_eq!(
        classify_cash_flow(&txn("cash", "loan_payable", "Loan"), &HashMap::new()),
        CashFlowActivity::Operating
    );
}

#[test]
fn test_closing_entry_to_retained_earnings_is_not_financing() {
    let accounts = standard_chart();
    assert_eq!(
        classify_cash_flow(
            &txn("4000", "3200", "Close sales to retained earnings"),
            &accounts
        ),
        CashFlowActivity::Operating
    );
}

#[test]
fn test_cash_flow_statement_sections_standard_chart_transactions() {
    let accounts = standard_chart();
    let transactions = [
        txn("1000", "3000", "Owner investment"),
        txn("1500", "1000", "Bought equipment"),
        txn("1000", "4000", "Cash sale"),
    ];

    let flow = cash_flow(day(), day(), &transactions, &accounts);

    assert_eq!(flow.financing_activities.len(), 1);
    assert_eq!(flow.investing_activities.len(), 1);
    assert_eq!(flow.operating_activities.len(), 1);
    assert_eq!(flow.net_cash_flow, BigDecimal::from(30));
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
