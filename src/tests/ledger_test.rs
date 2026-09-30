use crate::error::LedgerError;
use crate::ledger::{patterns, Ledger, STANDARD_CHART};
use crate::traits::AccountStore;
use crate::types::{AccountType, PaginatedResponse, PaginationOption, PaginationParams};
use crate::utils::memory_storage::MemoryStorage;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[tokio::test]
async fn test_ledger_basic_operations() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create accounts
    let cash_account = ledger
        .create_account(
            "cash".to_string(),
            "Cash".to_string(),
            AccountType::Asset,
            None,
        )
        .await
        .unwrap();

    let revenue_account = ledger
        .create_account(
            "revenue".to_string(),
            "Revenue".to_string(),
            AccountType::Income,
            None,
        )
        .await
        .unwrap();

    // Create a transaction
    let transaction = patterns::create_sales_transaction(
        "txn1".to_string(),
        chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Sale of goods".to_string(),
        cash_account.id.clone(),
        revenue_account.id.clone(),
        BigDecimal::from(1000),
    )
    .unwrap();

    // Record the transaction
    ledger.record_transaction(transaction).await.unwrap();

    // Check balances
    let cash_balance = ledger
        .get_account_balance(&cash_account.id, None)
        .await
        .unwrap();
    let revenue_balance = ledger
        .get_account_balance(&revenue_account.id, None)
        .await
        .unwrap();

    assert_eq!(cash_balance, BigDecimal::from(1000));
    assert_eq!(revenue_balance, BigDecimal::from(1000));

    // Generate reports
    let balance_sheet = ledger
        .generate_balance_sheet(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        .await
        .unwrap();

    assert_eq!(balance_sheet.total_assets, BigDecimal::from(1000));
}

#[tokio::test]
async fn test_unified_accounts_listing() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create multiple accounts
    for i in 1..=25 {
        ledger
            .create_account(
                format!("account_{i}"),
                format!("Test Account {i}"),
                AccountType::Asset,
                None,
            )
            .await
            .unwrap();
    }

    // Test getting all accounts without pagination
    let all_result = ledger.list_accounts(PaginationOption::All).await.unwrap();
    assert_eq!(all_result.items().len(), 25);
    assert!(!all_result.is_paginated());

    // Test pagination with default page size using unified API
    let pagination = PaginationParams::default(); // page: 1, page_size: 50
    let result = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    let result = result.into_paginated_response();

    assert_eq!(result.items.len(), 25); // All accounts fit in one page
    assert_eq!(result.total_count, 25);
    assert_eq!(result.page, 1);
    assert_eq!(result.page_size, 50);
    assert_eq!(result.total_pages, 1);
    assert!(!result.has_next);
    assert!(!result.has_previous);

    // Test unified API with paginated option
    let pagination_option = PaginationOption::Paginated(PaginationParams::new(1, 10).unwrap());
    let unified_result = ledger.list_accounts(pagination_option).await.unwrap();
    assert!(unified_result.is_paginated());
    assert_eq!(unified_result.items().len(), 10);

    // Test pagination with smaller page size using unified API
    let pagination = PaginationParams::new(1, 10).unwrap();
    let page1_response = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    let page1 = page1_response.into_paginated_response();

    assert_eq!(page1.items.len(), 10);
    assert_eq!(page1.total_count, 25);
    assert_eq!(page1.page, 1);
    assert_eq!(page1.page_size, 10);
    assert_eq!(page1.total_pages, 3);
    assert!(page1.has_next);
    assert!(!page1.has_previous);

    // Test second page
    let pagination = PaginationParams::new(2, 10).unwrap();
    let page2_response = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    let page2 = page2_response.into_paginated_response();

    assert_eq!(page2.items.len(), 10);
    assert_eq!(page2.total_count, 25);
    assert_eq!(page2.page, 2);
    assert!(page2.has_next);
    assert!(page2.has_previous);

    // Test last page
    let pagination = PaginationParams::new(3, 10).unwrap();
    let page3_response = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    let page3 = page3_response.into_paginated_response();

    assert_eq!(page3.items.len(), 5); // Remaining accounts
    assert_eq!(page3.total_count, 25);
    assert_eq!(page3.page, 3);
    assert!(!page3.has_next);
    assert!(page3.has_previous);

    // Verify no overlapping items between pages
    let page1_ids: Vec<_> = page1.items.iter().map(|a| &a.id).collect();
    let page2_ids: Vec<_> = page2.items.iter().map(|a| &a.id).collect();
    let page3_ids: Vec<_> = page3.items.iter().map(|a| &a.id).collect();

    // No IDs should overlap
    for id in &page1_ids {
        assert!(!page2_ids.contains(id));
        assert!(!page3_ids.contains(id));
    }
    for id in &page2_ids {
        assert!(!page3_ids.contains(id));
    }
}

#[tokio::test]
async fn test_unified_transactions_listing() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create accounts
    let cash_account = ledger
        .create_account(
            "cash".to_string(),
            "Cash".to_string(),
            AccountType::Asset,
            None,
        )
        .await
        .unwrap();

    let revenue_account = ledger
        .create_account(
            "revenue".to_string(),
            "Revenue".to_string(),
            AccountType::Income,
            None,
        )
        .await
        .unwrap();

    // Create multiple transactions
    for i in 1..=15 {
        let transaction = patterns::create_sales_transaction(
            format!("txn_{i}"),
            chrono::NaiveDate::from_ymd_opt(2024, 1, i).unwrap(),
            format!("Transaction {i}"),
            cash_account.id.clone(),
            revenue_account.id.clone(),
            BigDecimal::from(100 * i),
        )
        .unwrap();

        ledger.record_transaction(transaction).await.unwrap();
    }

    // Test getting all transactions without pagination
    let all_result = ledger
        .list_transactions(None, None, PaginationOption::All)
        .await
        .unwrap();
    assert_eq!(all_result.items().len(), 15);
    assert!(!all_result.is_paginated());

    // Test unified API with pagination
    let pagination_option = PaginationOption::Paginated(PaginationParams::new(1, 5).unwrap());
    let unified_result = ledger
        .list_transactions(None, None, pagination_option)
        .await
        .unwrap();
    assert!(unified_result.is_paginated());
    assert_eq!(unified_result.items().len(), 5);

    // Test pagination with default settings using unified API
    let pagination = PaginationParams::new(1, 5).unwrap();
    let result_response = ledger
        .list_transactions(None, None, PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    let result = result_response.into_paginated_response();

    assert_eq!(result.items.len(), 5);
    assert_eq!(result.total_count, 15);
    assert_eq!(result.page, 1);
    assert_eq!(result.page_size, 5);
    assert_eq!(result.total_pages, 3);
    assert!(result.has_next);
    assert!(!result.has_previous);

    // Verify transactions are sorted by date descending
    let dates: Vec<_> = result.items.iter().map(|t| t.date).collect();
    for i in 1..dates.len() {
        assert!(dates[i - 1] >= dates[i]);
    }

    // Test account-specific transactions pagination
    let pagination = PaginationParams::new(1, 10).unwrap();
    let account_result_response = ledger
        .list_account_transactions(
            &cash_account.id,
            None,
            None,
            PaginationOption::Paginated(pagination),
        )
        .await
        .unwrap();
    let account_result = account_result_response.into_paginated_response();

    assert_eq!(account_result.items.len(), 10);
    assert_eq!(account_result.total_count, 15);

    // All transactions should affect the cash account
    for txn in &account_result.items {
        assert!(txn.entries.iter().any(|e| e.account_id == cash_account.id));
    }
}

#[tokio::test]
async fn test_pagination_parameters_validation() {
    // Test invalid page number
    let result = PaginationParams::new(0, 10);
    assert!(result.is_err());

    // Test invalid page size (too small)
    let result = PaginationParams::new(1, 0);
    assert!(result.is_err());

    // Test invalid page size (too large)
    let result = PaginationParams::new(1, 1001);
    assert!(result.is_err());

    // Test valid parameters
    let result = PaginationParams::new(1, 50);
    assert!(result.is_ok());

    let params = result.unwrap();
    assert_eq!(params.offset(), 0);
    assert_eq!(params.limit(), 50);

    // Test offset calculation
    let params = PaginationParams::new(3, 20).unwrap();
    assert_eq!(params.offset(), 40); // (3-1) * 20
    assert_eq!(params.limit(), 20);
}

#[tokio::test]
async fn test_unified_api_optimization() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create test accounts
    for i in 1..=10 {
        ledger
            .create_account(
                format!("acc_{i}"),
                format!("Account {i}"),
                AccountType::Asset,
                None,
            )
            .await
            .unwrap();
    }

    // Test that single unified method can handle both cases

    // Case 1: Get all items (no pagination)
    let all_accounts = ledger.list_accounts(PaginationOption::All).await.unwrap();
    assert!(!all_accounts.is_paginated());
    assert_eq!(all_accounts.items().len(), 10);

    // Case 2: Get paginated results
    let pagination = PaginationParams::new(1, 5).unwrap();
    let paginated_accounts = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await
        .unwrap();
    assert!(paginated_accounts.is_paginated());
    assert_eq!(paginated_accounts.items().len(), 5);

    // Test conversion to PaginatedResponse for API compatibility
    let paginated_response = all_accounts.into_paginated_response();
    assert_eq!(paginated_response.items.len(), 10);
    assert_eq!(paginated_response.total_count, 10);
    assert_eq!(paginated_response.page, 1);
    assert_eq!(paginated_response.total_pages, 1);

    // Test convenience methods still work
    let convenience_all = ledger.list_all_accounts().await.unwrap();
    assert_eq!(convenience_all.len(), 10);
}

#[tokio::test]
async fn test_paginated_response_metadata() {
    // Test with exact division
    let response = PaginatedResponse::new(vec![1, 2, 3], 2, 3, 9);
    assert_eq!(response.total_pages, 3);
    assert!(response.has_previous);
    assert!(response.has_next);

    // Test first page
    let response = PaginatedResponse::new(vec![1, 2, 3], 1, 3, 9);
    assert!(!response.has_previous);
    assert!(response.has_next);

    // Test last page
    let response = PaginatedResponse::new(vec![7, 8, 9], 3, 3, 9);
    assert!(response.has_previous);
    assert!(!response.has_next);

    // Test single page
    let response = PaginatedResponse::new(vec![1, 2], 1, 5, 2);
    assert_eq!(response.total_pages, 1);
    assert!(!response.has_previous);
    assert!(!response.has_next);

    // Test empty result
    let response: PaginatedResponse<i32> = PaginatedResponse::new(vec![], 1, 10, 0);
    assert_eq!(response.total_pages, 1);
    assert!(!response.has_previous);
    assert!(!response.has_next);
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
}

async fn cash_and_revenue() -> Ledger<MemoryStorage> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger
        .create_account("cash", "Cash", AccountType::Asset, None)
        .await
        .unwrap();
    ledger
        .create_account("revenue", "Revenue", AccountType::Income, None)
        .await
        .unwrap();
    ledger
}

#[tokio::test]
async fn test_update_transaction_rebalances_accounts() {
    let mut ledger = cash_and_revenue().await;
    let sale = patterns::create_sales_transaction(
        "t1",
        day(1),
        "Sale",
        "cash",
        "revenue",
        BigDecimal::from(100),
    )
    .unwrap();
    ledger.record_transaction(sale).await.unwrap();

    let corrected = patterns::create_sales_transaction(
        "t1",
        day(1),
        "Sale",
        "cash",
        "revenue",
        BigDecimal::from(80),
    )
    .unwrap();
    ledger.update_transaction(&corrected).await.unwrap();

    let cash = ledger.get_account_balance("cash", None).await.unwrap();
    assert_eq!(cash, BigDecimal::from(80));

    ledger.delete_transaction("t1").await.unwrap();
    let cash = ledger.get_account_balance("cash", None).await.unwrap();
    assert_eq!(cash, BigDecimal::from(0));
}

#[tokio::test]
async fn test_update_transaction_rejects_unknown_accounts_without_side_effects() {
    let mut ledger = cash_and_revenue().await;
    let sale = patterns::create_sales_transaction(
        "t1",
        day(1),
        "Sale",
        "cash",
        "revenue",
        BigDecimal::from(100),
    )
    .unwrap();
    ledger.record_transaction(sale).await.unwrap();

    let bad = patterns::create_sales_transaction(
        "t1",
        day(1),
        "Sale",
        "cash",
        "missing",
        BigDecimal::from(100),
    )
    .unwrap();
    let result = ledger.update_transaction(&bad).await;

    assert!(matches!(result, Err(LedgerError::AccountNotFound(id)) if id == "missing"));
    let cash = ledger.get_account_balance("cash", None).await.unwrap();
    assert_eq!(cash, BigDecimal::from(100));
}

#[tokio::test]
async fn test_as_of_balance_matches_running_balance() {
    let mut ledger = cash_and_revenue().await;
    for (i, amount) in [(1, 100), (2, 50)] {
        let txn = patterns::create_sales_transaction(
            format!("t{i}"),
            day(i),
            "Sale",
            "cash",
            "revenue",
            BigDecimal::from(amount),
        )
        .unwrap();
        ledger.record_transaction(txn).await.unwrap();
    }

    let as_of_first = ledger.get_account_balance("cash", Some(day(1))).await;
    assert_eq!(as_of_first.unwrap(), BigDecimal::from(100));
    let as_of_later = ledger.get_account_balance("cash", Some(day(9))).await;
    let running = ledger.get_account_balance("cash", None).await;
    assert_eq!(as_of_later.unwrap(), running.unwrap());
}

#[tokio::test]
async fn test_account_hierarchy_navigation() {
    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger
        .create_account("assets", "Assets", AccountType::Asset, None)
        .await
        .unwrap();
    ledger
        .create_account(
            "current",
            "Current",
            AccountType::Asset,
            Some("assets".into()),
        )
        .await
        .unwrap();
    ledger
        .create_account("cash", "Cash", AccountType::Asset, Some("current".into()))
        .await
        .unwrap();

    let path: Vec<String> = ledger
        .account_path("cash")
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(path, ["assets", "current", "cash"]);

    let children = ledger.child_accounts("assets").await.unwrap();
    assert_eq!(children.len(), 1);

    let orphan = ledger
        .create_account("x", "X", AccountType::Asset, Some("nope".into()))
        .await;
    assert!(matches!(orphan, Err(LedgerError::ParentNotFound(_))));
    let duplicate = ledger
        .create_account("cash", "Cash", AccountType::Asset, None)
        .await;
    assert!(matches!(duplicate, Err(LedgerError::DuplicateAccount(_))));
}

#[tokio::test]
async fn test_standard_chart_of_accounts() {
    let mut ledger = Ledger::new(MemoryStorage::new());
    let chart = ledger.setup_standard_chart_of_accounts().await.unwrap();

    assert_eq!(chart.len(), STANDARD_CHART.len());
    assert_eq!(chart["cash"].id, "1000");
    assert_eq!(
        ledger.list_all_accounts().await.unwrap().len(),
        STANDARD_CHART.len()
    );
}

#[tokio::test]
async fn test_transactions_on_a_deleted_account_can_still_be_removed() {
    let mut ledger = cash_and_revenue().await;
    let sale = patterns::create_sales_transaction(
        "t1",
        day(1),
        "Sale",
        "cash",
        "revenue",
        BigDecimal::from(100),
    )
    .unwrap();
    ledger.record_transaction(sale.clone()).await.unwrap();
    // The ledger refuses to delete a used account, but a backend can still lose one
    let mut storage = ledger.into_storage();
    storage.delete_account("revenue").await.unwrap();
    let mut ledger = Ledger::new(storage);

    // Updating to entries on an account that is gone still fails...
    let result = ledger.update_transaction(&sale).await;
    assert!(matches!(result, Err(LedgerError::AccountNotFound(id)) if id == "revenue"));

    // ...but deleting reverses what can be reversed instead of getting stuck.
    ledger.delete_transaction("t1").await.unwrap();
    let cash = ledger.get_account_balance("cash", None).await.unwrap();
    assert_eq!(cash, BigDecimal::from(0));
    assert!(ledger.get_transaction("t1").await.unwrap().is_none());
}
