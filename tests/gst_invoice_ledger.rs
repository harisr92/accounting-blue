//! GST invoices and calculations wired into the ledger through the public API
#![cfg(feature = "india")]

use accounting_core::{
    utils::MemoryStorage, validate_invoice, AccountType, GstCalculator, GstCategory, GstInvoice,
    GstLineItem, Gstin, HsnMaster, InvoiceAccounts, Ledger, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

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
        "998311".to_string(),
        "Consulting Service".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(10000),
        GstCategory::Higher.rate(),
    )
    .unwrap();

    let invoice = GstInvoice::new(
        "INV-001".to_string(),
        NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
        Gstin::parse("27AAPFU0939F1ZV").unwrap(),
        Gstin::parse("27AAPFU0939F2ZU").unwrap(),
        vec![line_item],
    )
    .unwrap();
    let breakdown = invoice.breakdown().unwrap();

    // Record the invoice transaction
    let invoice_transaction = TransactionBuilder::new(
        "inv001".to_string(),
        NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
        "Invoice with GST".to_string(),
    )
    .debit(cash_account.id.clone(), breakdown.total.clone(), None)
    .credit(
        revenue_account.id.clone(),
        breakdown.taxable_value.clone(),
        None,
    )
    .credit(
        gst_payable_account.id.clone(),
        breakdown.total_tax.clone(),
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
async fn test_validated_invoice_posts_balanced_to_the_ledger() {
    let mut ledger = Ledger::new(MemoryStorage::new());
    let accounts = [
        ("receivable", AccountType::Asset),
        ("sales", AccountType::Income),
        ("cgst_output", AccountType::Liability),
        ("sgst_output", AccountType::Liability),
        ("igst_output", AccountType::Liability),
    ];
    for (id, account_type) in accounts {
        ledger
            .create_account(id, id, account_type, None)
            .await
            .unwrap();
    }

    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    let invoice = GstInvoice::new(
        "INV-002",
        date,
        Gstin::parse("27AAPFU0939F1ZV").unwrap(),
        Gstin::parse("27AAPFU0939F2ZU").unwrap(),
        vec![GstLineItem::with_default_rate(
            "998314",
            "IT consulting",
            BigDecimal::from(1),
            BigDecimal::from(10000),
        )
        .unwrap()],
    )
    .unwrap();

    let report = validate_invoice(&invoice, date, HsnMaster::global());
    assert!(report.issues().is_empty(), "{:?}", report.issues());

    let entries = invoice
        .to_entries(&InvoiceAccounts::new(
            "receivable",
            "sales",
            "cgst_output",
            "sgst_output",
            "igst_output",
        ))
        .unwrap();
    let transaction = entries
        .into_iter()
        .fold(
            TransactionBuilder::new("inv002", date, "Invoice INV-002")
                .reference(invoice.invoice_number.clone()),
            TransactionBuilder::entry,
        )
        .build()
        .unwrap();
    assert!(transaction.is_balanced());
    ledger.record_transaction(transaction).await.unwrap();

    for (id, expected) in [
        ("receivable", 11800),
        ("sales", 10000),
        ("cgst_output", 900),
        ("sgst_output", 900),
        ("igst_output", 0),
    ] {
        let balance = ledger.get_account_balance(id, None).await.unwrap();
        assert_eq!(balance, BigDecimal::from(expected), "{id}");
    }
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
