# Accounting Core

A comprehensive Rust library for double-entry bookkeeping, GST calculations, and financial reporting. Designed as the open-source foundation for accounting applications.

## Features

- **🏦 Double-entry Bookkeeping**: Complete transaction validation and balance tracking
- **📊 Account Management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
- **🧾 GST Calculations**: Indian GST compliance with CGST/SGST/IGST support
- **📄 Invoice PDFs**: Print-ready A4 tax invoices with Indian digit grouping and the total in words (optional `pdf` feature)
- **📑 GSTR-1 Returns**: Aggregate B2B invoices into GSTR-1 and export it as GST portal JSON
- **📈 Financial Reporting**: Balance sheets, income statements, and trial balance generation
- **🔗 Reconciliation**: Match ledger records against bank statements and payment gateways
- **🔍 Storage Abstraction**: Database-agnostic design with trait-based storage
- **✅ Validation**: Comprehensive validation for transactions and accounts
- **🧪 Testing**: Full test coverage with examples and documentation

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
accounting-core = "0.1.0"
```

### Basic Usage

```rust
use accounting_core::{Ledger, AccountType, TransactionBuilder};
use accounting_core::utils::MemoryStorage;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a ledger with in-memory storage
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Set up basic accounts
    let cash = ledger.create_account(
        "cash".to_string(),
        "Cash".to_string(),
        AccountType::Asset,
        None,
    ).await?;

    let revenue = ledger.create_account(
        "revenue".to_string(),
        "Revenue".to_string(),
        AccountType::Income,
        None,
    ).await?;

    // Record a transaction
    let transaction = TransactionBuilder::new(
        "txn001".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Sale of goods".to_string(),
    )
    .debit(cash.id.clone(), BigDecimal::from(1000), None)
    .credit(revenue.id.clone(), BigDecimal::from(1000), None)
    .build()?;

    ledger.record_transaction(transaction).await?;

    // Generate reports
    let balance_sheet = ledger.generate_balance_sheet(
        NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()
    ).await?;

    println!("Total Assets: {}", balance_sheet.total_assets);
    Ok(())
}
```

### GST Calculations

```rust
use accounting_core::{GstCalculator, GstCategory};
use bigdecimal::BigDecimal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let calculator = GstCalculator::new(false); // intra-state

    // Calculate GST for a service (18%)
    let calculation = calculator.calculate_by_category(
        BigDecimal::from(10000),
        GstCategory::Higher,
        None,
    )?;

    println!("Base Amount: ₹{}", calculation.base_amount);
    println!("CGST (9%): ₹{}", calculation.cgst_amount);
    println!("SGST (9%): ₹{}", calculation.sgst_amount);
    println!("Total: ₹{}", calculation.total_amount);

    Ok(())
}
```

### Reconciliation

Reconciliation matches internal ledger records against an external statement and tells you what
agrees, what does not, and what is probably the same transaction recorded slightly differently.

```rust
use accounting_core::reconciliation::{
    ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationEngine,
};
use accounting_core::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

let source = ExternalSource::BankStatement {
    bank_name: "SBI".to_string(),
    account_number: "12345678901".to_string(),
};

// Project the bank account's leg out of each double-entry transaction
let ledger_transactions: Vec<LedgerTransaction> = ledger
    .list_all_account_transactions("bank", Some(start), Some(end))
    .await?
    .iter()
    .filter_map(|txn| LedgerTransaction::from_transaction(txn, "bank"))
    .collect();

let report = ReconciliationEngine::default()
    .reconcile(&ledger_transactions, &external_transactions, source);

println!("Matched: {}", report.matched_count);
println!("Match rate: {:.1}%", report.summary.match_rate * 100.0);
println!("Balance difference: {}", report.summary.difference);

for item in report.needs_review() {
    println!("{item:?}");
}
```

Matching runs in four passes: shared reference numbers (UTR, RRN, cheque number), then exact
agreement on date, amount and direction, then a scored sweep of what remains assigned best-first,
and finally suggestions for whatever is still unmatched. Thresholds live in `ReconciliationConfig`.
The result never depends on the order of either input.

Two rules are deliberately strict, because they are accounting findings rather than noise: a pair
that disagrees on the **amount** or on the **direction** is always surfaced for review, however
well it scores.

`ReconciliationEngine` is pure and synchronous. Loading transactions and storing reports go
through the separate `ReconciliationStorage` trait, and statement parsing through
`ExternalDataParser` — this crate ships no parsers, since statement layouts differ per bank and
per gateway.

## Architecture

### Core Components

- **`types`**: Core data structures (Account, Transaction, Entry, etc.)
- **`traits`**: Storage and validation abstractions
- **`ledger`**: Account management and transaction processing
- **`tax`**: GST calculation engine
- **`invoice`**: GSTINs, B2B GST invoices, HSN/SAC master data, compliance checks and printing
- **`returns`**: GSTR-1 aggregation and export in the GST portal's JSON schema
- **`reconciliation`**: Matching engine for bank statements and payment gateways
- **`utils`**: Utilities including in-memory storage for testing

### Storage Abstraction

The library uses trait-based storage. A backend implements two small data-access traits, `AccountStore` and `TransactionStore`. Every type that implements both is automatically a `LedgerStorage` and can back a `Ledger`. Balances, trial balances and reports are computed by the library from the data you return, so a backend never has to implement accounting rules.

```rust
use accounting_core::{
    Account, AccountStore, AccountType, LedgerError, LedgerResult, ListResponse,
    PaginationOption, Transaction, TransactionFilter, TransactionStore,
};
use async_trait::async_trait;

pub struct MyPostgresStorage {
    pool: sqlx::PgPool,
}

#[async_trait]
impl AccountStore for MyPostgresStorage {
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()> {
        // Wrap backend failures so callers keep the cause:
        // sqlx::query(...).execute(&self.pool).await.map_err(LedgerError::storage)?;
        todo!()
    }

    // get_account, list_accounts, update_account, delete_account...
}

#[async_trait]
impl TransactionStore for MyPostgresStorage {
    // save_transaction, get_transaction, list_transactions(&TransactionFilter, PaginationOption),
    // update_transaction, delete_transaction...
}
```

Every error converts into `accounting_core::Error`, so application code can use one `?`-friendly `accounting_core::Result<T>` across the ledger, GST, invoice and reconciliation APIs.

## Examples

Run the examples to see the library in action:

```bash
# Basic ledger operations
cargo run --example basic_ledger

# GST calculation examples
cargo run --example gst_calculations

# GSTIN validation, HSN/SAC rates and B2B invoices
cargo run --example gst_invoice

# Render a GST invoice to PDF (pass a TTF with the ₹ glyph to print the rupee sign)
cargo run --example gst_invoice_pdf --features pdf

# Aggregate a month of invoices into GSTR-1 and print the portal JSON
cargo run --example gstr1_export

# Bank reconciliation
cargo run --example reconciliation
```

## Testing

Run the test suite:

```bash
cargo test
```

## Contributing

Contributions are welcome. Coding standards (functional core, SOLID, DRY, typed errors, no panics in library code) and the full list of verification commands are in [CLAUDE.md](CLAUDE.md). Breaking changes go in [CHANGELOG.md](CHANGELOG.md).

## Double-Entry Bookkeeping Principles

This library follows standard accounting principles:

- **Assets = Liabilities + Equity** (Balance Sheet equation)
- **Debits = Credits** (Every transaction must balance)
- **Account Types**:
  - **Assets**: Things the business owns (Cash, Inventory, Equipment)
  - **Liabilities**: Things the business owes (Loans, Accounts Payable)
  - **Equity**: Owner's interest in the business
  - **Income**: Revenue earned by the business
  - **Expenses**: Costs incurred by the business

### Normal Balances

- **Debit Balances**: Assets and Expenses
- **Credit Balances**: Liabilities, Equity, and Income

## GST Compliance

The library supports Indian GST with:

- **Intra-state transactions**: CGST + SGST
- **Inter-state transactions**: IGST
- **Standard rates**: 0%, 5%, 12%, 18%, 28%
- **Reverse calculations**: From total amount to base amount
- **Multi-item invoices**: Complex invoices with different rates
- **Unregistered buyers**: invoices to a `Recipient::Unregistered { place_of_supply }`, taxed by the place of supply
- **Exempt and non-GST supplies**: `GstLineItem::exempt` and `GstLineItem::non_gst` lines carry no GST, are reported in GSTR-1 Table 8, and an invoice of only such lines prints as a "Bill of Supply"

```rust,ignore
use accounting_core::invoice::{GstInvoice, Recipient, StateCode, SupplyKind};

let buyer = Recipient::unregistered(StateCode::parse("07")?); // Delhi, no GSTIN
let invoice = GstInvoice::new("INV-104", date, seller_gstin, buyer, items)?;
assert_eq!(invoice.supply_kind()?, SupplyKind::B2cl); // inter-state and over ₹1 lakh
```

### Invoice PDFs

Enable the `pdf` feature to render an invoice as an A4 PDF. It needs Rust 1.88 because of `printpdf`.

```toml
[dependencies]
accounting-core = { version = "0.2", features = ["pdf"] }
```

```rust,ignore
use accounting_core::invoice::{InvoiceParties, InvoiceParty, PdfOptions};

let parties = InvoiceParties::new(
    InvoiceParty::new("Acme Services", vec!["Mumbai".into()], seller_gstin),
    InvoiceParty::new("Globex Ltd", vec!["Bengaluru".into()], buyer_gstin),
);
let pdf: Vec<u8> = invoice.to_pdf(&parties, &PdfOptions::default())?;
```

The standard font has no `₹` glyph, so amounts are labelled `Rs.` by default. To print the rupee sign, pass a TrueType font that has it as `PdfFont::Custom` and set `currency_label` to `"₹"`. `InvoicePrint::from_invoice` gives the same formatted data without the `pdf` feature, for example to serialise as JSON. For an unregistered buyer, pass `InvoiceParty::unregistered(name, address)`. Below a taxable value of ₹50,000 (Rule 46), `InvoiceParty::walk_in()` prints the invoice without the buyer's name or address.

### GSTR-1

`Gstr1Return::build` aggregates a filer's invoices for one month into GSTR-1. It fills Table 4A (B2B supplies by buyer GSTIN, one item per rate), Table 5 (B2CL: inter-state supplies over ₹1 lakh to unregistered buyers, by place of supply), Table 7 (B2CS: all other supplies to unregistered buyers, summed by place of supply and rate), Table 8 (nil-rated, exempt and non-GST lines, for registered and unregistered buyers), Table 9B (credit notes: `cdnr` for registered buyers, `cdnur` against B2CL invoices), Table 12 (the HSN/SAC summary by code and rate) and Table 13 (documents issued). A credit note against a B2CS invoice is subtracted from Table 7, and every credit note is subtracted from Tables 8 and 12. `to_json` writes the portal's offline-tool schema, with amounts as numbers rounded to paise.

```rust,ignore
use accounting_core::invoice::HsnMaster;
use accounting_core::returns::{Gstr1Return, ReturnPeriod};

let gstr1 = Gstr1Return::build(&seller_gstin, ReturnPeriod::new(2024, 11)?, &invoices, &credit_notes, HsnMaster::global())?;
std::fs::write("gstr1.json", gstr1.to_json()?)?;
```

Every invoice and credit note must be issued by the filer, dated in the period, carry a unique number and pass the error-severity compliance checks; otherwise `build` returns a `Gstr1Error`. Supplies through an e-commerce operator, exports, debit notes and amendments are not covered yet.

## Financial Reports

Generate standard financial reports:

- **Trial Balance**: Verify that debits equal credits
- **Balance Sheet**: Assets = Liabilities + Equity
- **Income Statement**: Revenue - Expenses = Net Income
- **Cash Flow Statement**: Operating, Investing, Financing activities

## Validation

Comprehensive validation ensures data integrity:

- **Transaction validation**: Debits must equal credits
- **Account validation**: Proper account structure
- **Amount validation**: Positive amounts only
- **Reference validation**: Valid account references

## License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.


[Documentation](https://harisr92.github.io/accounting-blue/accounting_core/#reconciliation)

## Roadmap

- [x] Bank reconciliation engine
- [ ] Multi-currency support
- [ ] Advanced reporting features
- [ ] Plugin architecture
- [ ] Performance optimizations
- [ ] More comprehensive GST features
