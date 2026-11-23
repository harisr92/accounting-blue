//! A VAT invoice and a credit note against it, posted to the ledger
//!
//! Runs without any feature: invoicing is country-neutral. Swap `FlatRate::new("VAT")` for
//! `NoTax`, or implement `TaxRegime` for a tax system of your own.

use accounting_core::utils::MemoryStorage;
use accounting_core::{
    AccountType, CreditNote, Discount, Document, Entry, FlatRate, Invoice, Ledger, LineItem,
    SalesAccounts, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

/// Record `entries` as one transaction
async fn post(
    ledger: &mut Ledger<MemoryStorage>,
    id: &str,
    date: NaiveDate,
    entries: Vec<Entry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let transaction = entries
        .into_iter()
        .fold(
            TransactionBuilder::new(id, date, id),
            TransactionBuilder::entry,
        )
        .build()?;
    ledger.record_transaction(transaction).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - Invoicing Example\n");

    let mut ledger = Ledger::new(MemoryStorage::new());
    for (id, name, account_type) in [
        ("1200", "Accounts Receivable", AccountType::Asset),
        ("4000", "Sales", AccountType::Income),
        ("4100", "Sales Returns", AccountType::Income),
        ("2200", "VAT Payable", AccountType::Liability),
    ] {
        ledger.create_account(id, name, account_type, None).await?;
    }

    let date = |day| NaiveDate::from_ymd_opt(2025, 3, day).ok_or("bad date");
    let consulting = LineItem::new(
        "Consulting",
        BigDecimal::from(12),
        BigDecimal::from(90),
        BigDecimal::from(20),
    )?
    .with_unit("hours");
    let licence = LineItem::new(
        "Software licence",
        BigDecimal::from(1),
        BigDecimal::from(400),
        BigDecimal::from(5),
    )?;

    let invoice = Invoice::new(
        "INV-2025-001",
        date(1)?,
        "Acme Ltd",
        vec![consulting.clone(), licence],
        FlatRate::new("VAT"),
    )?
    .with_discount(Discount::Percent(BigDecimal::from(5)))?;

    let breakdown = invoice.tax_breakdown()?;
    println!("Invoice {} to {}", invoice.number, invoice.customer);
    println!("  Subtotal:  {}", invoice.subtotal());
    println!("  Discount:  {}", invoice.discount_amount());
    println!("  Taxable:   {}", breakdown.taxable_value);
    for component in &breakdown.components {
        println!("  {}:       {}", component.code, component.amount);
    }
    println!("  Total:     {}", breakdown.total);

    let sales = SalesAccounts::new("1200", "4000").with_output_tax("VAT", "2200");
    post(
        &mut ledger,
        "INV-2025-001",
        date(1)?,
        invoice.to_entries(&sales)?,
    )
    .await?;

    // Two hours are credited back
    let returned = LineItem {
        quantity: BigDecimal::from(2),
        ..consulting
    };
    let note = CreditNote::new(&invoice, "CN-2025-001", date(8)?, vec![returned])?;
    println!(
        "\nCredit note {} against {}: {}",
        note.number,
        note.original.number,
        note.tax_breakdown()?.total
    );
    let returns = SalesAccounts::new("1200", "4100").with_output_tax("VAT", "2200");
    post(
        &mut ledger,
        "CN-2025-001",
        date(8)?,
        note.to_entries(&returns)?,
    )
    .await?;

    println!("\nBalances:");
    for (id, name) in [
        ("1200", "Receivable"),
        ("4000", "Sales"),
        ("4100", "Returns"),
        ("2200", "VAT payable"),
    ] {
        println!(
            "  {name:<12} {}",
            ledger.get_account_balance(id, None).await?
        );
    }
    let trial_balance = ledger.get_trial_balance(date(31)?).await?;
    println!("\nTrial balance balanced: {}", trial_balance.is_balanced);
    Ok(())
}
