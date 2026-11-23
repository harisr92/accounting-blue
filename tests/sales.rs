//! Sales invoices and credit notes under a VAT regime, posted through the `Ledger`, with no
//! features

use accounting_core::document::validate_document;
use accounting_core::{
    utils::MemoryStorage, AccountType, CreditNote, Discount, Document, Entry, FlatRate, Invoice,
    Ledger, LineItem, SalesAccounts, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 3, day).unwrap()
}

fn chairs(quantity: i32) -> LineItem {
    LineItem::new(
        "Office chair",
        BigDecimal::from(quantity),
        BigDecimal::from(150),
        BigDecimal::from(20),
    )
    .unwrap()
    .with_unit("pcs")
}

fn accounts() -> SalesAccounts {
    SalesAccounts::new("receivable", "sales").with_output_tax("VAT", "vat_payable")
}

async fn books() -> Ledger<MemoryStorage> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    for (id, account_type) in [
        ("receivable", AccountType::Asset),
        ("sales", AccountType::Income),
        ("vat_payable", AccountType::Liability),
    ] {
        ledger
            .create_account(id, id, account_type, None)
            .await
            .unwrap();
    }
    ledger
}

async fn post(ledger: &mut Ledger<MemoryStorage>, id: &str, day: u32, entries: Vec<Entry>) {
    let transaction = entries
        .into_iter()
        .fold(
            TransactionBuilder::new(id, date(day), id),
            TransactionBuilder::entry,
        )
        .build()
        .unwrap();
    ledger.record_transaction(transaction).await.unwrap();
}

async fn balance(ledger: &Ledger<MemoryStorage>, account: &str) -> BigDecimal {
    ledger.get_account_balance(account, None).await.unwrap()
}

#[tokio::test]
async fn test_vat_invoice_and_credit_note_post_balanced_and_net_out() {
    let invoice = Invoice::new(
        "INV-001",
        date(1),
        "Acme Ltd",
        vec![chairs(10)],
        FlatRate::new("VAT"),
    )
    .unwrap()
    .with_discount(Discount::Amount(BigDecimal::from(100)))
    .unwrap();
    assert!(validate_document(&invoice, date(1)).is_compliant());
    assert_eq!(
        invoice.tax_breakdown().unwrap().total,
        BigDecimal::from(1680)
    );

    let note = CreditNote::new(&invoice, "CN-001", date(5), vec![chairs(2)]).unwrap();

    let mut ledger = books().await;
    post(
        &mut ledger,
        "INV-001",
        1,
        invoice.to_entries(&accounts()).unwrap(),
    )
    .await;
    post(
        &mut ledger,
        "CN-001",
        5,
        note.to_entries(&accounts()).unwrap(),
    )
    .await;

    // Invoice: 1400 + 280 VAT; note: 300 + 60 VAT back
    assert_eq!(balance(&ledger, "receivable").await, BigDecimal::from(1320));
    assert_eq!(balance(&ledger, "sales").await, BigDecimal::from(1100));
    assert_eq!(balance(&ledger, "vat_payable").await, BigDecimal::from(220));

    let trial_balance = ledger.get_trial_balance(date(31)).await.unwrap();
    assert!(trial_balance.is_balanced);
    assert!(ledger.validate_integrity(date(31)).await.unwrap().is_valid);
}
