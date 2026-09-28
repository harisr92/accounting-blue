//! Comprehensive examples of using paginated responses for accounts and transactions
//!
//! This example demonstrates:
//! - Basic pagination setup
//! - Navigating through pages
//! - Filtering with pagination
//! - Building pagination UI helpers
//! - Error handling

use accounting_core::{
    utils::memory_storage::MemoryStorage, AccountType, Entry, EntryType, Ledger, PaginationOption,
    PaginationParams, Transaction,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Accounting Core - Pagination Examples\n");

    // Create a ledger with in-memory storage
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Setup sample data
    setup_sample_data(&mut ledger).await?;

    // Example 1: Basic account pagination
    println!("📄 Example 1: Basic Account Pagination");
    basic_account_pagination(&ledger).await?;
    println!();

    // Example 2: Paginated accounts by type
    println!("🏢 Example 2: Paginated Accounts by Type");
    paginated_accounts_by_type(&ledger).await?;
    println!();

    // Example 3: Basic transaction pagination
    println!("💸 Example 3: Basic Transaction Pagination");
    basic_transaction_pagination(&ledger).await?;
    println!();

    // Example 4: Account-specific transaction pagination
    println!("🏦 Example 4: Account-Specific Transaction Pagination");
    account_transaction_pagination(&ledger).await?;
    println!();

    // Example 5: Date-filtered transaction pagination
    println!("📅 Example 5: Date-Filtered Transaction Pagination");
    date_filtered_pagination(&ledger).await?;
    println!();

    // Example 6: Pagination navigation helpers
    println!("🧭 Example 6: Pagination Navigation Helpers");
    pagination_navigation_helpers(&ledger).await?;
    println!();

    // Example 7: Error handling and validation
    println!("⚠️  Example 7: Error Handling and Validation");
    error_handling_examples().await?;
    println!();

    // Example 8: Building a simple pagination UI
    println!("🖥️  Example 8: Simple Pagination UI Helper");
    pagination_ui_helper(&ledger).await?;

    Ok(())
}

/// Setup sample data for demonstrations
async fn setup_sample_data(
    ledger: &mut Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Setting up sample data...");

    // Create various account types
    let account_data = [
        ("cash", "Cash", AccountType::Asset),
        ("bank", "Bank Account", AccountType::Asset),
        ("inventory", "Inventory", AccountType::Asset),
        ("equipment", "Equipment", AccountType::Asset),
        (
            "accounts_receivable",
            "Accounts Receivable",
            AccountType::Asset,
        ),
        ("prepaid_expenses", "Prepaid Expenses", AccountType::Asset),
        (
            "accounts_payable",
            "Accounts Payable",
            AccountType::Liability,
        ),
        ("loan_payable", "Loan Payable", AccountType::Liability),
        (
            "accrued_expenses",
            "Accrued Expenses",
            AccountType::Liability,
        ),
        ("owner_equity", "Owner's Equity", AccountType::Equity),
        (
            "retained_earnings",
            "Retained Earnings",
            AccountType::Equity,
        ),
        ("sales_revenue", "Sales Revenue", AccountType::Income),
        ("service_revenue", "Service Revenue", AccountType::Income),
        ("interest_income", "Interest Income", AccountType::Income),
        ("rent_expense", "Rent Expense", AccountType::Expense),
        (
            "utilities_expense",
            "Utilities Expense",
            AccountType::Expense,
        ),
        ("salary_expense", "Salary Expense", AccountType::Expense),
        (
            "marketing_expense",
            "Marketing Expense",
            AccountType::Expense,
        ),
    ];

    // Create accounts
    for (id, name, account_type) in account_data {
        ledger
            .create_account(id.to_string(), name.to_string(), account_type, None)
            .await?;
    }

    // Create sample transactions
    for i in 1..=30 {
        let mut transaction = Transaction::new(
            format!("txn_{i:03}"),
            NaiveDate::from_ymd_opt(2024, 1, (i % 28) + 1).unwrap(),
            format!("Sample transaction {i}"),
            Some(format!("REF{i:03}")),
        );

        // Create various transaction patterns
        match i % 4 {
            0 => {
                // Sales transaction: Debit Cash, Credit Sales Revenue
                transaction.add_entry(Entry::debit(
                    "cash".to_string(),
                    BigDecimal::from(1000 + i * 10),
                    Some("Cash received".to_string()),
                ));
                transaction.add_entry(Entry::credit(
                    "sales_revenue".to_string(),
                    BigDecimal::from(1000 + i * 10),
                    Some("Sales revenue".to_string()),
                ));
            }
            1 => {
                // Expense payment: Debit Rent Expense, Credit Cash
                transaction.add_entry(Entry::debit(
                    "rent_expense".to_string(),
                    BigDecimal::from(500 + i * 5),
                    Some("Rent payment".to_string()),
                ));
                transaction.add_entry(Entry::credit(
                    "cash".to_string(),
                    BigDecimal::from(500 + i * 5),
                    Some("Cash payment".to_string()),
                ));
            }
            2 => {
                // Equipment purchase: Debit Equipment, Credit Cash
                transaction.add_entry(Entry::debit(
                    "equipment".to_string(),
                    BigDecimal::from(2000 + i * 20),
                    Some("Equipment purchase".to_string()),
                ));
                transaction.add_entry(Entry::credit(
                    "cash".to_string(),
                    BigDecimal::from(2000 + i * 20),
                    Some("Cash payment".to_string()),
                ));
            }
            _ => {
                // Service revenue: Debit Accounts Receivable, Credit Service Revenue
                transaction.add_entry(Entry::debit(
                    "accounts_receivable".to_string(),
                    BigDecimal::from(800 + i * 8),
                    Some("Service rendered".to_string()),
                ));
                transaction.add_entry(Entry::credit(
                    "service_revenue".to_string(),
                    BigDecimal::from(800 + i * 8),
                    Some("Service revenue".to_string()),
                ));
            }
        }

        ledger.record_transaction(transaction).await?;
    }

    println!("✅ Sample data created: 18 accounts, 30 transactions\n");
    Ok(())
}

/// Example 1: Basic account pagination
async fn basic_account_pagination(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get first page with default settings (page 1, 50 items per page)
    let pagination = PaginationParams::default();
    let result = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    println!("📊 First page (default settings):");
    println!("   Items: {}/{}", result.items.len(), result.total_count);
    println!("   Page: {} of {}", result.page, result.total_pages);
    println!(
        "   Has next: {}, Has previous: {}",
        result.has_next, result.has_previous
    );

    // Show first 3 accounts
    for account in result.items.iter().take(3) {
        println!("   • {} - {}", account.id, account.name);
    }
    if result.items.len() > 3 {
        println!("   ... and {} more", result.items.len() - 3);
    }

    // Get first page with smaller page size
    let pagination = PaginationParams::new(1, 5)?;
    let result = ledger
        .list_accounts(PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    println!("\n📊 First page (5 items per page):");
    println!("   Items: {}/{}", result.items.len(), result.total_count);
    println!("   Page: {} of {}", result.page, result.total_pages);

    for account in &result.items {
        println!(
            "   • {} - {} ({:?})",
            account.id, account.name, account.account_type
        );
    }

    Ok(())
}

/// Example 2: Paginated accounts by type
async fn paginated_accounts_by_type(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get only Asset accounts with pagination
    let pagination = PaginationParams::new(1, 10)?;
    let result = ledger
        .list_accounts_by_type(AccountType::Asset, PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    println!("📊 Asset accounts (page 1):");
    println!("   Total assets: {}", result.total_count);
    println!("   Showing: {} items", result.items.len());

    for account in &result.items {
        println!("   💰 {} - {}", account.id, account.name);
    }

    // Get Revenue accounts
    let pagination2 = PaginationParams::new(1, 10)?;
    let result = ledger
        .list_accounts_by_type(
            AccountType::Income,
            PaginationOption::Paginated(pagination2),
        )
        .await?;
    let result = result.into_paginated_response();

    println!("\n📊 Income accounts:");
    println!("   Total income accounts: {}", result.total_count);

    for account in &result.items {
        println!("   💵 {} - {}", account.id, account.name);
    }

    Ok(())
}

/// Example 3: Basic transaction pagination
async fn basic_transaction_pagination(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get first page of transactions (10 per page)
    let pagination = PaginationParams::new(1, 10)?;
    let result = ledger
        .list_transactions(None, None, PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    println!("📊 Recent transactions (page 1 of {}):", result.total_pages);
    println!("   Total transactions: {}", result.total_count);

    for transaction in &result.items {
        println!(
            "   📝 {} - {} ({})",
            transaction.id, transaction.description, transaction.date
        );
        println!("      Amount: ${}", transaction.total_debits());
    }

    // Get second page
    if result.has_next {
        println!("\n📊 Getting second page...");
        let pagination = PaginationParams::new(2, 10)?;
        let result = ledger
            .list_transactions(None, None, PaginationOption::Paginated(pagination))
            .await?;
        let result = result.into_paginated_response();

        println!("   Page 2 - {} transactions:", result.items.len());
        for transaction in result.items.iter().take(3) {
            println!("   📝 {} - {}", transaction.id, transaction.description);
        }
        if result.items.len() > 3 {
            println!("   ... and {} more", result.items.len() - 3);
        }
    }

    Ok(())
}

/// Example 4: Account-specific transaction pagination
async fn account_transaction_pagination(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get transactions for the cash account
    let pagination = PaginationParams::new(1, 5)?;
    let result = ledger
        .list_account_transactions("cash", None, None, PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    println!("📊 Cash account transactions:");
    println!("   Total: {} transactions", result.total_count);
    println!("   Page: {} of {}", result.page, result.total_pages);

    for transaction in &result.items {
        // Find the cash entry in this transaction
        let cash_entry = transaction
            .entries
            .iter()
            .find(|e| e.account_id == "cash")
            .unwrap();

        let direction = match cash_entry.entry_type {
            EntryType::Debit => "💰 Received",
            EntryType::Credit => "💸 Paid",
        };

        println!(
            "   {} ${} - {} ({})",
            direction, cash_entry.amount, transaction.description, transaction.date
        );
    }

    Ok(())
}

/// Example 5: Date-filtered transaction pagination
async fn date_filtered_pagination(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get transactions for the first week of January 2024
    let start_date = NaiveDate::from_ymd_opt(2024, 1, 1);
    let end_date = NaiveDate::from_ymd_opt(2024, 1, 7);
    let pagination = PaginationParams::new(1, 20)?;

    let result = ledger
        .list_transactions(
            start_date,
            end_date,
            PaginationOption::Paginated(pagination),
        )
        .await?;
    let paginated_result = result.into_paginated_response();

    println!("📊 Transactions from Jan 1-7, 2024:");
    println!("   Found: {} transactions", paginated_result.total_count);

    for transaction in &paginated_result.items {
        println!(
            "   📅 {} - {} (${:.2})",
            transaction.date,
            transaction.description,
            transaction.total_debits()
        );
    }

    // Get transactions for mid-January
    let start_date = NaiveDate::from_ymd_opt(2024, 1, 15);
    let end_date = NaiveDate::from_ymd_opt(2024, 1, 20);
    let pagination2 = PaginationParams::new(1, 20)?;

    let result = ledger
        .list_transactions(
            start_date,
            end_date,
            PaginationOption::Paginated(pagination2),
        )
        .await?;
    let paginated_result = result.into_paginated_response();

    println!("\n📊 Transactions from Jan 15-20, 2024:");
    println!("   Found: {} transactions", paginated_result.total_count);

    for transaction in &paginated_result.items {
        println!("   📅 {} - {}", transaction.date, transaction.description);
    }

    Ok(())
}

/// Example 6: Pagination navigation helpers
async fn pagination_navigation_helpers(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let page_size = 8;
    let mut current_page = 1;

    println!("🧭 Navigation through all account pages:");

    loop {
        let pagination = PaginationParams::new(current_page, page_size)?;
        let result = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?;
        let paginated_result = result.into_paginated_response();

        println!(
            "\n   📄 Page {} of {} ({} items):",
            paginated_result.page,
            paginated_result.total_pages,
            paginated_result.items.len()
        );

        for (i, account) in paginated_result.items.iter().enumerate() {
            println!(
                "      {}. {} - {}",
                (current_page - 1) * page_size + i as u32 + 1,
                account.id,
                account.name
            );
        }

        // Navigation info
        let nav_info = build_navigation_info(&paginated_result);
        println!("   🔗 Navigation: {nav_info}");

        // Break after showing 2 pages as example
        if current_page >= 2 {
            println!("   (Showing first 2 pages as example...)");
            break;
        }

        if !paginated_result.has_next {
            break;
        }

        current_page += 1;
    }

    Ok(())
}

/// Example 7: Error handling and validation
async fn error_handling_examples() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚠️  Testing validation errors:");

    // Test invalid page number
    match PaginationParams::new(0, 10) {
        Ok(_) => println!("   ❌ Should have failed for page 0"),
        Err(e) => println!("   ✅ Page 0 rejected: {e}"),
    }

    // Test invalid page size (too small)
    match PaginationParams::new(1, 0) {
        Ok(_) => println!("   ❌ Should have failed for page_size 0"),
        Err(e) => println!("   ✅ Page size 0 rejected: {e}"),
    }

    // Test invalid page size (too large)
    match PaginationParams::new(1, 1001) {
        Ok(_) => println!("   ❌ Should have failed for page_size 1001"),
        Err(e) => println!("   ✅ Page size 1001 rejected: {e}"),
    }

    // Test valid parameters
    match PaginationParams::new(5, 25) {
        Ok(params) => {
            println!(
                "   ✅ Valid params: page={}, size={}",
                params.page, params.page_size
            );
            println!(
                "      Offset: {}, Limit: {}",
                params.offset(),
                params.limit()
            );
        }
        Err(e) => println!("   ❌ Unexpected error: {e}"),
    }

    Ok(())
}

/// Example 8: Simple pagination UI helper
async fn pagination_ui_helper(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🖥️  Pagination UI Helper Example:");

    // Simulate a UI request for page 2 of transactions
    let page_request = 2;
    let page_size = 6;

    let pagination = PaginationParams::new(page_request, page_size)?;
    let result = ledger
        .list_transactions(None, None, PaginationOption::Paginated(pagination))
        .await?;
    let result = result.into_paginated_response();

    // Build UI-friendly response
    let ui_response = PaginationUIResponse {
        items: result
            .items
            .iter()
            .map(|t| TransactionSummary {
                id: t.id.clone(),
                date: t.date,
                description: t.description.clone(),
                amount: t.total_debits(),
            })
            .collect(),
        pagination_info: PaginationInfo {
            current_page: result.page,
            total_pages: result.total_pages,
            page_size: result.page_size,
            total_items: result.total_count,
            has_previous: result.has_previous,
            has_next: result.has_next,
            start_item: ((result.page - 1) * result.page_size) + 1,
            end_item: std::cmp::min(result.page * result.page_size, result.total_count),
        },
    };

    println!("   📊 Transaction List (UI Format):");
    println!(
        "   Showing items {}-{} of {}",
        ui_response.pagination_info.start_item,
        ui_response.pagination_info.end_item,
        ui_response.pagination_info.total_items
    );

    for item in &ui_response.items {
        println!(
            "   • {} - {} (${:.2})",
            item.date, item.description, item.amount
        );
    }

    // Generate pagination controls
    let controls = generate_pagination_controls(&ui_response.pagination_info);
    println!("\n   🎛️  Pagination Controls:");
    println!("   {controls}");

    Ok(())
}

// Helper structures for UI examples
#[derive(Debug)]
#[allow(dead_code)]
struct TransactionSummary {
    id: String,
    date: NaiveDate,
    description: String,
    amount: BigDecimal,
}

#[derive(Debug)]
#[allow(dead_code)]
struct PaginationInfo {
    current_page: u32,
    total_pages: u32,
    page_size: u32,
    total_items: u32,
    has_previous: bool,
    has_next: bool,
    start_item: u32,
    end_item: u32,
}

#[derive(Debug)]
struct PaginationUIResponse {
    items: Vec<TransactionSummary>,
    pagination_info: PaginationInfo,
}

/// Build navigation information string
fn build_navigation_info<T>(result: &accounting_core::PaginatedResponse<T>) -> String {
    let mut parts = Vec::new();

    if result.has_previous {
        parts.push("← Prev".to_string());
    }

    parts.push(format!("Page {} of {}", result.page, result.total_pages));

    if result.has_next {
        parts.push("Next →".to_string());
    }

    parts.join(" | ")
}

/// Generate pagination controls for UI
fn generate_pagination_controls(info: &PaginationInfo) -> String {
    let mut controls = Vec::new();

    // Previous button
    if info.has_previous {
        controls.push("[← Previous]".to_string());
    } else {
        controls.push("[Previous]".to_string());
    }

    // Page numbers (show current and adjacent)
    let start_page = if info.current_page > 2 {
        info.current_page - 2
    } else {
        1
    };
    let end_page = std::cmp::min(start_page + 4, info.total_pages);

    if start_page > 1 {
        controls.push("[1]".to_string());
        if start_page > 2 {
            controls.push("...".to_string());
        }
    }

    for page in start_page..=end_page {
        if page == info.current_page {
            controls.push(format!("[{page}]"));
        } else {
            controls.push(format!("{page}"));
        }
    }

    if end_page < info.total_pages {
        if end_page < info.total_pages - 1 {
            controls.push("...".to_string());
        }
        controls.push(format!("[{}]", info.total_pages));
    }

    // Next button
    if info.has_next {
        controls.push("[Next →]".to_string());
    } else {
        controls.push("[Next]".to_string());
    }

    controls.join(" ")
}
