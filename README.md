# Accounting Core

A comprehensive Rust library for double-entry bookkeeping, GST calculations, and financial reporting. Designed as the open-source foundation for accounting applications.

## Features

- **🏦 Double-entry Bookkeeping**: Complete transaction validation and balance tracking
- **📊 Account Management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
- **🧾 GST Calculations**: Indian GST compliance with CGST/SGST/IGST support
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
use accounting_core::{GstCalculator, GstCategory, GstLineItem, GstInvoice};
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
    .get_all_account_transactions("bank", Some(start), Some(end))
    .await?
    .iter()
    .filter_map(|txn| LedgerTransaction::from_transaction(txn, "bank"))
    .collect();

let report = ReconciliationEngine::default()
    .reconcile(ledger_transactions, external_transactions, source);

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
- **`reconciliation`**: Matching engine for bank statements and payment gateways
- **`utils`**: Utilities including in-memory storage for testing

### Storage Abstraction

The library uses trait-based storage abstraction, allowing you to implement your own storage backend:

```rust
use accounting_core::{LedgerStorage, Account, Transaction};
use async_trait::async_trait;

#[derive(Debug)]
pub struct MyPostgresStorage {
    pool: sqlx::PgPool,
}

#[async_trait]
impl LedgerStorage for MyPostgresStorage {
    async fn save_account(&mut self, account: &Account) -> LedgerResult<()> {
        // Your PostgreSQL implementation
        todo!()
    }
    
    // Implement other required methods...
}
```

## Examples

Run the examples to see the library in action:

```bash
# Basic ledger operations
cargo run --example basic_ledger

# GST calculation examples
cargo run --example gst_calculations

# Bank reconciliation
cargo run --example reconciliation
```

## Testing

Run the test suite:

```bash
cargo test
```

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

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

[Documentation](https://harisr92.github.io/accounting-blue/accounting_core/#reconciliation)

## Roadmap

- [x] Bank reconciliation engine
- [ ] Multi-currency support
- [ ] Advanced reporting features
- [ ] Plugin architecture
- [ ] Performance optimizations
- [ ] More comprehensive GST features
