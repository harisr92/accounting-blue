//! Paginated listing of accounts and transactions through the `Ledger`

use accounting_core::{
    utils::MemoryStorage, AccountType, Ledger, LedgerError, LedgerResult, PaginationOption,
    PaginationParams, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, month, day).unwrap()
}

fn page(number: u32, size: u32) -> PaginationOption {
    PaginationOption::Paginated(PaginationParams::new(number, size).unwrap())
}

/// A standard chart plus one cash sale on the 1st of each month from January to June, and a
/// rent payment from the bank on the 15th of each of the first three months
async fn ledger_with_history() -> LedgerResult<Ledger<MemoryStorage>> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger.setup_standard_chart_of_accounts().await?;
    ledger
        .create_account("bank", "Bank", AccountType::Asset, None)
        .await?;

    for month in 1..=6 {
        let sale = TransactionBuilder::new(format!("sale-{month}"), date(month, 1), "Sale")
            .debit("1000", BigDecimal::from(100 * month), None)
            .credit("4000", BigDecimal::from(100 * month), None)
            .build()?;
        ledger.record_transaction(sale).await?;
    }
    for month in 1..=3 {
        let rent = TransactionBuilder::new(format!("rent-{month}"), date(month, 15), "Rent")
            .debit("6000", BigDecimal::from(50), None)
            .credit("bank", BigDecimal::from(50), None)
            .build()?;
        ledger.record_transaction(rent).await?;
    }
    Ok(ledger)
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<String> {
    items.iter().map(|item| id(item).to_string()).collect()
}

#[tokio::test]
async fn test_accounts_page_through_in_id_order() -> LedgerResult<()> {
    let ledger = ledger_with_history().await?;

    let first = ledger
        .list_accounts(page(1, 5))
        .await?
        .into_paginated_response();
    assert_eq!(
        ids(&first.items, |a| &a.id),
        ["1000", "1200", "1300", "2000", "2100"]
    );
    assert_eq!(first.total_count, 13);
    assert_eq!(first.total_pages, 3);
    assert!(first.has_next);
    assert!(!first.has_previous);

    let last = ledger
        .list_accounts(page(3, 5))
        .await?
        .into_paginated_response();
    assert_eq!(ids(&last.items, |a| &a.id), ["6000", "6100", "bank"]);
    assert_eq!(last.page, 3);
    assert!(!last.has_next);
    assert!(last.has_previous);

    let beyond = ledger
        .list_accounts(page(4, 5))
        .await?
        .into_paginated_response();
    assert!(beyond.items.is_empty());
    assert_eq!(beyond.total_count, 13);
    Ok(())
}

#[tokio::test]
async fn test_accounts_filter_by_type_before_paging() -> LedgerResult<()> {
    let ledger = ledger_with_history().await?;

    let assets = ledger
        .list_accounts_by_type(AccountType::Asset, page(1, 3))
        .await?
        .into_paginated_response();
    assert_eq!(ids(&assets.items, |a| &a.id), ["1000", "1200", "1300"]);
    assert_eq!(assets.total_count, 4);
    assert_eq!(assets.total_pages, 2);

    let expenses = ledger
        .list_accounts_by_type(AccountType::Expense, PaginationOption::All)
        .await?;
    assert_eq!(ids(expenses.items(), |a| &a.id), ["5000", "6000", "6100"]);
    Ok(())
}

#[tokio::test]
async fn test_transactions_page_newest_first_within_a_date_range() -> LedgerResult<()> {
    let ledger = ledger_with_history().await?;

    let q1 = ledger
        .list_transactions(Some(date(1, 1)), Some(date(3, 31)), page(1, 4))
        .await?
        .into_paginated_response();
    assert_eq!(
        ids(&q1.items, |t| &t.id),
        ["rent-3", "sale-3", "rent-2", "sale-2"]
    );
    assert_eq!(q1.total_count, 6);
    assert_eq!(q1.total_pages, 2);

    let rest = ledger
        .list_transactions(Some(date(1, 1)), Some(date(3, 31)), page(2, 4))
        .await?
        .into_paginated_response();
    assert_eq!(ids(&rest.items, |t| &t.id), ["rent-1", "sale-1"]);

    // Both bounds are inclusive, and either can be left open
    let from_june = ledger
        .list_transactions(Some(date(6, 1)), None, PaginationOption::All)
        .await?;
    assert_eq!(ids(from_june.items(), |t| &t.id), ["sale-6"]);
    let to_jan_first = ledger
        .list_transactions(None, Some(date(1, 1)), PaginationOption::All)
        .await?;
    assert_eq!(ids(to_jan_first.items(), |t| &t.id), ["sale-1"]);
    Ok(())
}

#[tokio::test]
async fn test_account_transactions_list_only_that_account() -> LedgerResult<()> {
    let ledger = ledger_with_history().await?;

    let bank = ledger
        .list_account_transactions("bank", None, None, page(1, 2))
        .await?
        .into_paginated_response();
    assert_eq!(ids(&bank.items, |t| &t.id), ["rent-3", "rent-2"]);
    assert_eq!(bank.total_count, 3);
    assert!(bank.has_next);

    let cash_feb_to_apr = ledger
        .list_account_transactions(
            "1000",
            Some(date(2, 1)),
            Some(date(4, 30)),
            PaginationOption::All,
        )
        .await?;
    assert_eq!(
        ids(cash_feb_to_apr.items(), |t| &t.id),
        ["sale-4", "sale-3", "sale-2"]
    );

    let untouched = ledger
        .list_account_transactions("1300", None, None, page(1, 10))
        .await?
        .into_paginated_response();
    assert!(untouched.items.is_empty());
    assert_eq!(untouched.total_count, 0);
    assert!(!untouched.has_next);
    Ok(())
}

#[test]
fn test_out_of_range_pagination_is_rejected() {
    assert!(matches!(
        PaginationParams::new(0, 10),
        Err(LedgerError::InvalidPagination(_))
    ));
    assert!(matches!(
        PaginationParams::new(1, 0),
        Err(LedgerError::InvalidPagination(_))
    ));
    assert!(matches!(
        PaginationParams::new(1, accounting_core::types::MAX_PAGE_SIZE + 1),
        Err(LedgerError::InvalidPagination(_))
    ));
    assert!(PaginationParams::new(1, accounting_core::types::MAX_PAGE_SIZE).is_ok());
}
