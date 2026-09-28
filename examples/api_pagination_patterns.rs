//! Practical API pagination patterns for web applications
//!
//! This example shows how to integrate pagination into REST APIs,
//! handle query parameters, and build efficient data access patterns.

use accounting_core::{
    utils::memory_storage::MemoryStorage, Account, AccountType, Ledger, PaginationOption,
    PaginationParams,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 API Pagination Patterns Example\n");

    // Setup
    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);
    setup_sample_accounts(&mut ledger).await?;

    // Simulate different API request patterns
    rest_api_simulation(&ledger).await?;
    graphql_pagination_pattern(&ledger).await?;
    infinite_scroll_pattern(&ledger).await?;
    table_pagination_pattern(&ledger).await?;

    Ok(())
}

/// Setup sample accounts for API demonstrations
async fn setup_sample_accounts(
    ledger: &mut Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a realistic chart of accounts
    let accounts = [
        // Assets
        ("1000", "Cash - Checking", AccountType::Asset),
        ("1010", "Cash - Savings", AccountType::Asset),
        ("1100", "Accounts Receivable", AccountType::Asset),
        ("1200", "Inventory", AccountType::Asset),
        ("1500", "Equipment", AccountType::Asset),
        (
            "1510",
            "Accumulated Depreciation - Equipment",
            AccountType::Asset,
        ),
        ("1600", "Building", AccountType::Asset),
        (
            "1610",
            "Accumulated Depreciation - Building",
            AccountType::Asset,
        ),
        // Liabilities
        ("2000", "Accounts Payable", AccountType::Liability),
        ("2100", "Short-term Loan", AccountType::Liability),
        ("2200", "Long-term Loan", AccountType::Liability),
        ("2300", "Accrued Expenses", AccountType::Liability),
        // Equity
        ("3000", "Owner's Capital", AccountType::Equity),
        ("3100", "Retained Earnings", AccountType::Equity),
        // Income
        ("4000", "Sales Revenue", AccountType::Income),
        ("4100", "Service Revenue", AccountType::Income),
        ("4200", "Interest Income", AccountType::Income),
        // Expenses
        ("5000", "Cost of Goods Sold", AccountType::Expense),
        ("5100", "Rent Expense", AccountType::Expense),
        ("5200", "Salary Expense", AccountType::Expense),
        ("5300", "Utilities Expense", AccountType::Expense),
        ("5400", "Marketing Expense", AccountType::Expense),
        ("5500", "Office Supplies", AccountType::Expense),
        ("5600", "Professional Services", AccountType::Expense),
        ("5700", "Insurance Expense", AccountType::Expense),
    ];

    for (id, name, account_type) in &accounts {
        ledger
            .create_account(id.to_string(), name.to_string(), *account_type, None)
            .await?;
    }

    println!("✅ Created {} sample accounts", accounts.len());
    Ok(())
}

/// Simulate REST API request/response patterns
async fn rest_api_simulation(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 REST API Simulation\n");

    // Example 1: GET /api/accounts?page=1&limit=10&type=asset
    println!("📞 Request: GET /api/accounts?page=1&limit=10&type=asset");
    let result = api_get_accounts(
        ledger,
        ApiAccountsQuery {
            page: Some(1),
            limit: Some(10),
            account_type: Some("asset".to_string()),
        },
    )
    .await?;

    println!("📤 Response:");
    println!("{}", serde_json::to_string_pretty(&result)?);

    println!("\n{}\n", "─".repeat(60));

    // Example 2: GET /api/accounts?page=2&limit=5
    println!("📞 Request: GET /api/accounts?page=2&limit=5");
    let result = api_get_accounts(
        ledger,
        ApiAccountsQuery {
            page: Some(2),
            limit: Some(5),
            account_type: None,
        },
    )
    .await?;

    println!("📤 Response (metadata only):");
    println!("   Total: {}", result.meta.total);
    println!(
        "   Page: {} of {}",
        result.meta.page, result.meta.total_pages
    );
    println!("   Items: {}", result.data.len());

    Ok(())
}

/// Simulate GraphQL-style pagination
async fn graphql_pagination_pattern(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 GraphQL Pagination Pattern\n");

    // Simulate cursor-based pagination query
    println!("📞 GraphQL Query:");
    println!("   accounts(first: 5, after: null, filter: {{ type: EXPENSE }}) {{");
    println!("     edges {{ node {{ id, name, type }} }}");
    println!("     pageInfo {{ hasNextPage, hasPreviousPage }}");
    println!("   }}");

    let result = graphql_get_accounts(
        ledger,
        GraphQLAccountsQuery {
            first: Some(5),
            after: None,
            filter: Some(GraphQLAccountFilter {
                account_type: Some("EXPENSE".to_string()),
            }),
        },
    )
    .await?;

    println!("\n📤 GraphQL Response:");
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

/// Simulate infinite scroll pagination
async fn infinite_scroll_pattern(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("♾️  Infinite Scroll Pattern\n");

    let mut loaded_items = 0;
    let batch_size = 6;

    // Simulate loading 3 batches
    for batch in 1..=3 {
        println!("📱 Loading batch {batch} (infinite scroll):");

        let pagination = PaginationParams::new(batch, batch_size)?;
        let result = ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?;
        let result = result.into_paginated_response();

        loaded_items += result.items.len();

        println!(
            "   ↓ Loaded {} items (total: {}/{})",
            result.items.len(),
            loaded_items,
            result.total_count
        );

        // Show last 2 items in this batch
        for account in result.items.iter().rev().take(2).rev() {
            println!("     • {} - {}", account.id, account.name);
        }

        if !result.has_next {
            println!("   ✅ All items loaded!");
            break;
        }

        println!("   📜 Scroll for more...\n");
    }

    Ok(())
}

/// Simulate data table pagination with sorting and filtering
async fn table_pagination_pattern(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Data Table Pagination\n");

    // Simulate table request with sorting and filtering
    let table_request = TableRequest {
        page: 1,
        page_size: 8,
        sort_by: "name".to_string(),
        sort_order: "asc".to_string(),
        filters: {
            let mut filters = HashMap::new();
            filters.insert("type".to_string(), "liability".to_string());
            filters
        },
    };

    println!("🗂️  Table Request:");
    println!(
        "   Page: {}, Size: {}",
        table_request.page, table_request.page_size
    );
    println!(
        "   Sort: {} {}",
        table_request.sort_by, table_request.sort_order
    );
    println!("   Filters: {:?}", table_request.filters);

    let result = process_table_request(ledger, table_request).await?;

    println!("\n📊 Table Response:");
    println!("┌─────────┬──────────────────────────────┬─────────────┐");
    println!("│ Account │ Name                         │ Type        │");
    println!("├─────────┼──────────────────────────────┼─────────────┤");

    for account in &result.data {
        println!(
            "│ {:7} │ {:28} │ {:11} │",
            account.id,
            &account.name[..std::cmp::min(account.name.len(), 28)],
            format!("{:?}", account.account_type)
        );
    }

    println!("└─────────┴──────────────────────────────┴─────────────┘");
    println!(
        "Showing {} of {} items | Page {} of {}",
        result.data.len(),
        result.pagination.total_items,
        result.pagination.current_page,
        result.pagination.total_pages
    );

    Ok(())
}

// API Data Structures
#[derive(Debug, Deserialize)]
struct ApiAccountsQuery {
    page: Option<u32>,
    limit: Option<u32>,
    account_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiAccountsResponse {
    data: Vec<AccountDto>,
    meta: ApiPaginationMeta,
}

#[derive(Debug, Serialize)]
struct AccountDto {
    id: String,
    name: String,
    #[serde(rename = "type")]
    account_type: String,
    balance: String,
}

#[derive(Debug, Serialize)]
struct ApiPaginationMeta {
    page: u32,
    limit: u32,
    total: u32,
    total_pages: u32,
    has_next: bool,
    has_previous: bool,
}

// GraphQL structures
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GraphQLAccountsQuery {
    first: Option<u32>,
    after: Option<String>,
    filter: Option<GraphQLAccountFilter>,
}

#[derive(Debug, Deserialize)]
struct GraphQLAccountFilter {
    account_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct GraphQLAccountsResponse {
    data: GraphQLAccountsData,
}

#[derive(Debug, Serialize)]
struct GraphQLAccountsData {
    accounts: GraphQLAccountConnection,
}

#[derive(Debug, Serialize)]
struct GraphQLAccountConnection {
    edges: Vec<GraphQLAccountEdge>,
    #[serde(rename = "pageInfo")]
    page_info: GraphQLPageInfo,
}

#[derive(Debug, Serialize)]
struct GraphQLAccountEdge {
    node: AccountDto,
}

#[derive(Debug, Serialize)]
struct GraphQLPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "hasPreviousPage")]
    has_previous_page: bool,
}

// Table structures
#[derive(Debug)]
struct TableRequest {
    page: u32,
    page_size: u32,
    sort_by: String,
    sort_order: String,
    filters: HashMap<String, String>,
}

#[derive(Debug)]
struct TableResponse {
    data: Vec<Account>,
    pagination: TablePaginationInfo,
}

#[derive(Debug)]
#[allow(dead_code)]
struct TablePaginationInfo {
    current_page: u32,
    total_pages: u32,
    total_items: u32,
    page_size: u32,
}

// API Implementation Functions

async fn api_get_accounts(
    ledger: &Ledger<MemoryStorage>,
    query: ApiAccountsQuery,
) -> Result<ApiAccountsResponse, Box<dyn std::error::Error>> {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(20);
    let pagination = PaginationParams::new(page, limit)?;

    let result = if let Some(type_str) = query.account_type {
        let account_type = match type_str.to_lowercase().as_str() {
            "asset" => AccountType::Asset,
            "liability" => AccountType::Liability,
            "equity" => AccountType::Equity,
            "income" => AccountType::Income,
            "expense" => AccountType::Expense,
            _ => return Err("Invalid account type".into()),
        };
        ledger
            .list_accounts_by_type(account_type, PaginationOption::Paginated(pagination))
            .await?
    } else {
        ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?
    };
    let result = result.into_paginated_response();

    let data = result
        .items
        .into_iter()
        .map(|account| AccountDto {
            id: account.id,
            name: account.name,
            account_type: format!("{:?}", account.account_type).to_lowercase(),
            balance: account.balance.to_string(),
        })
        .collect();

    Ok(ApiAccountsResponse {
        data,
        meta: ApiPaginationMeta {
            page: result.page,
            limit: result.page_size,
            total: result.total_count,
            total_pages: result.total_pages,
            has_next: result.has_next,
            has_previous: result.has_previous,
        },
    })
}

async fn graphql_get_accounts(
    ledger: &Ledger<MemoryStorage>,
    query: GraphQLAccountsQuery,
) -> Result<GraphQLAccountsResponse, Box<dyn std::error::Error>> {
    let first = query.first.unwrap_or(10);
    let page = 1; // In a real implementation, you'd decode the cursor

    let pagination = PaginationParams::new(page, first)?;

    let result = if let Some(filter) = query.filter {
        if let Some(type_str) = filter.account_type {
            let account_type = match type_str.as_str() {
                "ASSET" => AccountType::Asset,
                "LIABILITY" => AccountType::Liability,
                "EQUITY" => AccountType::Equity,
                "INCOME" => AccountType::Income,
                "EXPENSE" => AccountType::Expense,
                _ => return Err("Invalid account type".into()),
            };
            ledger
                .list_accounts_by_type(account_type, PaginationOption::Paginated(pagination))
                .await?
        } else {
            ledger
                .list_accounts(PaginationOption::Paginated(pagination))
                .await?
        }
    } else {
        ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?
    };
    let result = result.into_paginated_response();

    let edges = result
        .items
        .into_iter()
        .map(|account| GraphQLAccountEdge {
            node: AccountDto {
                id: account.id,
                name: account.name,
                account_type: format!("{:?}", account.account_type),
                balance: account.balance.to_string(),
            },
        })
        .collect();

    Ok(GraphQLAccountsResponse {
        data: GraphQLAccountsData {
            accounts: GraphQLAccountConnection {
                edges,
                page_info: GraphQLPageInfo {
                    has_next_page: result.has_next,
                    has_previous_page: result.has_previous,
                },
            },
        },
    })
}

async fn process_table_request(
    ledger: &Ledger<MemoryStorage>,
    request: TableRequest,
) -> Result<TableResponse, Box<dyn std::error::Error>> {
    let pagination = PaginationParams::new(request.page, request.page_size)?;

    let result = if let Some(type_filter) = request.filters.get("type") {
        let account_type = match type_filter.as_str() {
            "asset" => AccountType::Asset,
            "liability" => AccountType::Liability,
            "equity" => AccountType::Equity,
            "income" => AccountType::Income,
            "expense" => AccountType::Expense,
            _ => return Err("Invalid account type filter".into()),
        };
        ledger
            .list_accounts_by_type(account_type, PaginationOption::Paginated(pagination))
            .await?
    } else {
        ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?
    };
    let result = result.into_paginated_response();

    // Note: In a real implementation, you'd handle sorting at the storage level
    // For this example, we'll just use the results as-is since they're already sorted by ID

    Ok(TableResponse {
        data: result.items,
        pagination: TablePaginationInfo {
            current_page: result.page,
            total_pages: result.total_pages,
            total_items: result.total_count,
            page_size: result.page_size,
        },
    })
}
