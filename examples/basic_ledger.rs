//! Basic ledger usage example

use accounting_core::utils::MemoryStorage;
use accounting_core::{
    patterns, AccountType, GstCalculator, GstCategory, Ledger, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - Basic Ledger Example\n");

    // Create a new ledger with in-memory storage
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // 1. Set up a basic chart of accounts
    println!("📊 Setting up Chart of Accounts...");
    let accounts = ledger.setup_standard_chart_of_accounts().await?;

    for account in accounts.values() {
        println!(
            "  ✓ Created account: {} - {} ({:?})",
            account.id, account.name, account.account_type
        );
    }
    println!();

    // 2. Record some business transactions
    println!("💰 Recording Business Transactions...\n");

    // Owner invests cash in the business
    let investment = patterns::create_owner_investment(
        "txn001".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        "Initial owner investment".to_string(),
        accounts["cash"].id.clone(),
        accounts["owners_equity"].id.clone(),
        BigDecimal::from(50000),
    )?;

    ledger.record_transaction(investment).await?;
    println!("  ✓ Recorded: Owner investment of ₹50,000");

    // Purchase equipment - create an equipment account first
    let equipment_account = ledger
        .create_account(
            "equipment".to_string(),
            "Equipment".to_string(),
            AccountType::Asset,
            None,
        )
        .await?;

    let equipment_purchase = TransactionBuilder::new(
        "txn002".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
        "Purchase of office equipment".to_string(),
    )
    .debit(
        equipment_account.id.clone(),
        BigDecimal::from(15000),
        Some("Office computer and printer".to_string()),
    )
    .credit(accounts["cash"].id.clone(), BigDecimal::from(15000), None)
    .build()?;

    ledger.record_transaction(equipment_purchase).await?;
    println!("  ✓ Recorded: Equipment purchase of ₹15,000");

    // Create GST payable account for sales
    let gst_account = ledger
        .create_account(
            "gst_payable".to_string(),
            "GST Payable".to_string(),
            AccountType::Liability,
            None,
        )
        .await?;

    // Make a sale with GST
    println!("\n🧾 Processing Sale with GST...");
    let gst_calculator = GstCalculator::new(false); // intra-state
    let sale_calculation = gst_calculator.calculate_by_category(
        BigDecimal::from(10000),
        GstCategory::Higher, // 18%
        None,
    )?;

    println!("  Sale Amount: ₹{}", sale_calculation.base_amount);
    println!("  CGST (9%):   ₹{}", sale_calculation.cgst_amount);
    println!("  SGST (9%):   ₹{}", sale_calculation.sgst_amount);
    println!("  Total GST:   ₹{}", sale_calculation.total_gst_amount);
    println!("  Total:       ₹{}", sale_calculation.total_amount);

    // Record the sale transaction
    let sale_transaction = TransactionBuilder::new(
        "txn003".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
        "Sale of goods with GST".to_string(),
    )
    .debit(
        accounts["accounts_receivable"].id.clone(),
        sale_calculation.total_amount.clone(),
        None,
    )
    .credit(
        accounts["sales_revenue"].id.clone(),
        sale_calculation.base_amount.clone(),
        None,
    )
    .credit(
        gst_account.id.clone(),
        sale_calculation.total_gst_amount.clone(),
        None,
    )
    .build()?;

    ledger.record_transaction(sale_transaction).await?;
    println!("  ✓ Recorded: Sale transaction with GST");

    // Pay some expenses
    let rent_payment = patterns::create_expense_payment(
        "txn004".to_string(),
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        "Monthly rent payment".to_string(),
        accounts["rent_expense"].id.clone(),
        accounts["cash"].id.clone(),
        BigDecimal::from(8000),
    )?;

    ledger.record_transaction(rent_payment).await?;
    println!("  ✓ Recorded: Rent payment of ₹8,000");

    // 3. Generate financial reports
    println!("\n📈 Generating Financial Reports...\n");

    // Trial Balance
    let trial_balance = ledger
        .get_trial_balance(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await?;

    println!("🔍 Trial Balance as of January 31, 2024:");
    println!("  Total Debits:  ₹{}", trial_balance.total_debits);
    println!("  Total Credits: ₹{}", trial_balance.total_credits);
    println!(
        "  Balanced: {}",
        if trial_balance.is_balanced {
            "✅ Yes"
        } else {
            "❌ No"
        }
    );
    println!();

    // Balance Sheet
    let balance_sheet = ledger
        .generate_balance_sheet(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await?;

    println!("📊 Balance Sheet as of January 31, 2024:");
    println!("  Assets:");
    for asset in &balance_sheet.assets {
        println!("    {}: ₹{}", asset.account.name, asset.balance_amount());
    }
    println!("  Total Assets: ₹{}", balance_sheet.total_assets);
    println!();

    println!("  Liabilities:");
    for liability in &balance_sheet.liabilities {
        println!(
            "    {}: ₹{}",
            liability.account.name,
            liability.balance_amount()
        );
    }
    println!("  Total Liabilities: ₹{}", balance_sheet.total_liabilities);
    println!();

    println!("  Equity:");
    for equity in &balance_sheet.equity {
        println!("    {}: ₹{}", equity.account.name, equity.balance_amount());
    }
    println!("  Total Equity: ₹{}", balance_sheet.total_equity);
    println!();

    println!(
        "  Balanced: {}",
        if balance_sheet.is_balanced {
            "✅ Yes"
        } else {
            "❌ No"
        }
    );

    // Income Statement
    let income_statement = ledger
        .generate_income_statement(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        )
        .await?;

    println!("\n💹 Income Statement for January 2024:");
    println!("  Revenue:");
    for revenue in &income_statement.revenue {
        println!(
            "    {}: ₹{}",
            revenue.account.name,
            revenue.balance_amount()
        );
    }
    println!("  Total Revenue: ₹{}", income_statement.total_revenue);
    println!();

    println!("  Expenses:");
    for expense in &income_statement.expenses {
        println!(
            "    {}: ₹{}",
            expense.account.name,
            expense.balance_amount()
        );
    }
    println!("  Total Expenses: ₹{}", income_statement.total_expenses);
    println!();

    println!("  Net Income: ₹{}", income_statement.net_income);

    // 4. Validate ledger integrity
    println!("\n🔍 Validating Ledger Integrity...");
    let integrity_report = ledger
        .validate_integrity(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        .await?;

    if integrity_report.is_valid {
        println!("  ✅ Ledger integrity check passed!");
    } else {
        println!("  ❌ Ledger integrity check failed:");
        for issue in &integrity_report.issues {
            println!("    - {issue}");
        }
    }

    println!("\n🎉 Example completed successfully!");
    Ok(())
}
