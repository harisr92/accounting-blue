//! Integration tests for accounting-core

use accounting_core::{
    patterns,
    utils::{EnhancedAccountValidator, EnhancedTransactionValidator, MemoryStorage},
    AccountType, GstCalculator, GstCategory, GstInvoice, GstLineItem, Ledger, LedgerStorage,
    TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[tokio::test]
async fn test_complete_accounting_workflow() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Set up chart of accounts
    let accounts = ledger.setup_standard_chart_of_accounts().await.unwrap();

    // Verify accounts were created
    assert!(accounts.contains_key("cash"));
    assert!(accounts.contains_key("sales_revenue"));
    assert!(accounts.contains_key("owners_equity"));

    // Record initial investment
    let investment = patterns::create_owner_investment(
        "invest1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Initial investment".to_string(),
        accounts["cash"].id.clone(),
        accounts["owners_equity"].id.clone(),
        BigDecimal::from(100000),
    )
    .unwrap();

    ledger.record_transaction(investment).await.unwrap();

    // Check cash balance
    let cash_balance = ledger
        .get_account_balance(&accounts["cash"].id, None)
        .await
        .unwrap();
    assert_eq!(cash_balance, BigDecimal::from(100000));

    // Record a sale
    let sale = patterns::create_sales_transaction(
        "sale1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
        "First sale".to_string(),
        accounts["cash"].id.clone(),
        accounts["sales_revenue"].id.clone(),
        BigDecimal::from(15000),
    )
    .unwrap();

    ledger.record_transaction(sale).await.unwrap();

    // Check updated cash balance
    let updated_cash_balance = ledger
        .get_account_balance(&accounts["cash"].id, None)
        .await
        .unwrap();
    assert_eq!(updated_cash_balance, BigDecimal::from(115000));

    // Generate trial balance
    let trial_balance = ledger
        .get_trial_balance(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await
        .unwrap();
    assert!(trial_balance.is_balanced);

    // Generate balance sheet
    let balance_sheet = ledger
        .generate_balance_sheet(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await
        .unwrap();
    assert!(balance_sheet.is_balanced);
    assert_eq!(balance_sheet.total_assets, BigDecimal::from(115000));

    // Validate integrity
    let integrity_report = ledger
        .validate_integrity(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await
        .unwrap();
    assert!(integrity_report.is_valid);
}

#[tokio::test]
async fn test_gst_invoice_with_ledger_integration() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Set up basic accounts
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

    let gst_payable_account = ledger
        .create_account(
            "gst_payable".to_string(),
            "GST Payable".to_string(),
            AccountType::Liability,
            None,
        )
        .await
        .unwrap();

    // Create GST invoice
    let line_item = GstLineItem::new(
        "Consulting Service".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(10000),
        GstCategory::Higher.intra_state_rate(),
    )
    .unwrap();

    let invoice = GstInvoice::new(vec![line_item]);

    // Record the invoice transaction
    let invoice_transaction = TransactionBuilder::new(
        "inv001".to_string(),
        NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
        "Invoice with GST".to_string(),
    )
    .debit(cash_account.id.clone(), invoice.grand_total.clone(), None)
    .credit(
        revenue_account.id.clone(),
        invoice.total_before_gst.clone(),
        None,
    )
    .credit(
        gst_payable_account.id.clone(),
        invoice.total_gst.clone(),
        None,
    )
    .build()
    .unwrap();

    ledger
        .record_transaction(invoice_transaction)
        .await
        .unwrap();

    // Verify balances
    let cash_balance = ledger
        .get_account_balance(&cash_account.id, None)
        .await
        .unwrap();
    let revenue_balance = ledger
        .get_account_balance(&revenue_account.id, None)
        .await
        .unwrap();
    let gst_balance = ledger
        .get_account_balance(&gst_payable_account.id, None)
        .await
        .unwrap();

    assert_eq!(cash_balance, BigDecimal::from(11800)); // 10000 + 1800 GST
    assert_eq!(revenue_balance, BigDecimal::from(10000));
    assert_eq!(gst_balance, BigDecimal::from(1800)); // 18% GST
}

#[tokio::test]
async fn test_transaction_validation() {
    let storage = MemoryStorage::new();
    let validator = Box::new(EnhancedTransactionValidator);
    let mut ledger =
        Ledger::with_validators(storage, Box::new(EnhancedAccountValidator), validator);

    // Create test accounts
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

    // Test valid transaction
    let valid_transaction = TransactionBuilder::new(
        "valid1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Valid transaction".to_string(),
    )
    .debit(cash_account.id.clone(), BigDecimal::from(1000), None)
    .credit(revenue_account.id.clone(), BigDecimal::from(1000), None)
    .build()
    .unwrap();

    let result = ledger.record_transaction(valid_transaction).await;
    assert!(result.is_ok());

    // Test unbalanced transaction
    let unbalanced_transaction = TransactionBuilder::new(
        "invalid1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Unbalanced transaction".to_string(),
    )
    .debit(cash_account.id.clone(), BigDecimal::from(1000), None)
    .credit(revenue_account.id.clone(), BigDecimal::from(500), None) // Unbalanced!
    .build();

    assert!(unbalanced_transaction.is_err());
}

#[tokio::test]
async fn test_account_hierarchy() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create parent account
    let parent_account = ledger
        .create_account(
            "current_assets".to_string(),
            "Current Assets".to_string(),
            AccountType::Asset,
            None,
        )
        .await
        .unwrap();

    // Create child account
    let child_account = ledger
        .create_account(
            "cash".to_string(),
            "Cash".to_string(),
            AccountType::Asset,
            Some(parent_account.id.clone()),
        )
        .await
        .unwrap();

    assert_eq!(child_account.parent_id, Some(parent_account.id));

    // Test listing accounts by type
    let asset_accounts = ledger
        .list_accounts_by_type(AccountType::Asset)
        .await
        .unwrap();
    assert_eq!(asset_accounts.len(), 2);
}

#[tokio::test]
async fn test_date_range_filtering() {
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Set up accounts
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

    // Record transactions on different dates
    let txn1 = TransactionBuilder::new(
        "txn1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "January transaction".to_string(),
    )
    .debit(cash_account.id.clone(), BigDecimal::from(1000), None)
    .credit(revenue_account.id.clone(), BigDecimal::from(1000), None)
    .build()
    .unwrap();

    let txn2 = TransactionBuilder::new(
        "txn2".to_string(),
        NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
        "February transaction".to_string(),
    )
    .debit(cash_account.id.clone(), BigDecimal::from(2000), None)
    .credit(revenue_account.id.clone(), BigDecimal::from(2000), None)
    .build()
    .unwrap();

    ledger.record_transaction(txn1).await.unwrap();
    ledger.record_transaction(txn2).await.unwrap();

    // Test date range filtering
    let jan_transactions = ledger
        .get_transactions(
            Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            Some(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()),
        )
        .await
        .unwrap();

    assert_eq!(jan_transactions.len(), 1);
    assert_eq!(jan_transactions[0].id, "txn1");

    // Test account balance as of specific date
    let jan_balance = ledger
        .get_account_balance(
            &cash_account.id,
            Some(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()),
        )
        .await
        .unwrap();

    let feb_balance = ledger
        .get_account_balance(
            &cash_account.id,
            Some(NaiveDate::from_ymd_opt(2024, 2, 28).unwrap()),
        )
        .await
        .unwrap();

    assert_eq!(jan_balance, BigDecimal::from(1000));
    assert_eq!(feb_balance, BigDecimal::from(3000));
}

#[test]
fn test_gst_calculations() {
    // Test intra-state GST
    let intra_calc = GstCalculator::new(false)
        .calculate_by_category(BigDecimal::from(1000), GstCategory::Higher, None)
        .unwrap();

    assert_eq!(intra_calc.base_amount, BigDecimal::from(1000));
    assert_eq!(intra_calc.cgst_amount, BigDecimal::from(90));
    assert_eq!(intra_calc.sgst_amount, BigDecimal::from(90));
    assert_eq!(intra_calc.igst_amount, BigDecimal::from(0));
    assert_eq!(intra_calc.total_gst_amount, BigDecimal::from(180));

    // Test inter-state GST
    let inter_calc = GstCalculator::new(true)
        .calculate_by_category(BigDecimal::from(1000), GstCategory::Higher, None)
        .unwrap();

    assert_eq!(inter_calc.base_amount, BigDecimal::from(1000));
    assert_eq!(inter_calc.cgst_amount, BigDecimal::from(0));
    assert_eq!(inter_calc.sgst_amount, BigDecimal::from(0));
    assert_eq!(inter_calc.igst_amount, BigDecimal::from(180));
    assert_eq!(inter_calc.total_gst_amount, BigDecimal::from(180));
}

#[tokio::test]
async fn test_memory_storage_operations() {
    let mut storage = MemoryStorage::new();

    // Test account operations
    let account = accounting_core::Account::new(
        "test1".to_string(),
        "Test Account".to_string(),
        AccountType::Asset,
        None,
    );

    storage.save_account(&account).await.unwrap();

    let retrieved = storage.get_account("test1").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Account");

    let all_accounts = storage.list_accounts(None).await.unwrap();
    assert_eq!(all_accounts.len(), 1);

    // Test transaction operations
    let transaction = TransactionBuilder::new(
        "txn1".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Test transaction".to_string(),
    )
    .debit("test1".to_string(), BigDecimal::from(100), None)
    .credit("test2".to_string(), BigDecimal::from(100), None)
    .build()
    .unwrap();

    storage.save_transaction(&transaction).await.unwrap();

    let retrieved_txn = storage.get_transaction("txn1").await.unwrap();
    assert!(retrieved_txn.is_some());
    assert_eq!(retrieved_txn.unwrap().description, "Test transaction");
}
