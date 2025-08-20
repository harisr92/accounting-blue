# Accounting Core

A comprehensive Rust library for double-entry bookkeeping, GST calculations, and financial reporting. Designed as the open-source foundation for accounting applications.

## Features

- **🏦 Double-entry Bookkeeping**: Complete transaction validation and balance tracking
- **📊 Account Management**: Support for Assets, Liabilities, Equity, Income, and Expense accounts
- **🧾 GST Calculations**: Indian GST compliance with CGST/SGST/IGST support
- **📈 Financial Reporting**: Balance sheets, income statements, and trial balance generation
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

## Architecture

### Core Components

- **`types`**: Core data structures (Account, Transaction, Entry, etc.)
- **`traits`**: Storage and validation abstractions
- **`ledger`**: Account management and transaction processing
- **`tax`**: GST calculation engine
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

## Roadmap

- [ ] Bank reconciliation engine
- [ ] Multi-currency support
- [ ] Advanced reporting features
- [ ] Plugin architecture
- [ ] Performance optimizations
- [ ] More comprehensive GST features