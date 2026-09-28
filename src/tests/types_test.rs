use crate::types::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

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
