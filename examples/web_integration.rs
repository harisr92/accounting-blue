//! Web Framework Integration Examples
//!
//! This example demonstrates how to integrate pagination with popular web frameworks
//! including error handling, query parameter parsing, and response formatting.

use accounting_core::{
    utils::memory_storage::MemoryStorage, AccountType, Ledger, PaginationOption, PaginationParams,
};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 Web Framework Integration Examples\n");

    let storage = MemoryStorage::new();
    let mut ledger = Ledger::new(storage);

    // Create sample data
    create_sample_data(&mut ledger).await?;

    // Demonstrate different web framework patterns
    axum_style_handlers(&ledger).await?;
    actix_style_handlers(&ledger).await?;
    warp_style_handlers(&ledger).await?;

    Ok(())
}

async fn create_sample_data(
    ledger: &mut Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create 15 sample accounts for demonstration
    let accounts = [
        ("cash", "Cash Account", AccountType::Asset),
        ("bank", "Bank Account", AccountType::Asset),
        ("inventory", "Inventory", AccountType::Asset),
        ("equipment", "Office Equipment", AccountType::Asset),
        ("building", "Office Building", AccountType::Asset),
        (
            "accounts_payable",
            "Accounts Payable",
            AccountType::Liability,
        ),
        ("loan", "Business Loan", AccountType::Liability),
        ("mortgage", "Building Mortgage", AccountType::Liability),
        ("equity", "Owner's Equity", AccountType::Equity),
        ("retained", "Retained Earnings", AccountType::Equity),
        ("sales", "Sales Revenue", AccountType::Income),
        ("services", "Service Revenue", AccountType::Income),
        ("rent", "Rent Expense", AccountType::Expense),
        ("utilities", "Utilities Expense", AccountType::Expense),
        ("marketing", "Marketing Expense", AccountType::Expense),
    ];

    for (id, name, account_type) in accounts {
        ledger
            .create_account(id.to_string(), name.to_string(), account_type, None)
            .await?;
    }

    Ok(())
}

/// Axum-style request handlers
async fn axum_style_handlers(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🦀 Axum-Style Handler Examples");
    println!("{}", "=".repeat(40));

    // Simulate Axum query parameters
    println!("\n📞 GET /accounts?page=2&per_page=5&type=asset");

    let query_params = AxumAccountsQuery {
        page: Some(2),
        per_page: Some(5),
        account_type: Some("asset".to_string()),
        sort: None,
        order: None,
    };

    match axum_list_accounts(ledger, query_params).await {
        Ok(response) => {
            println!("✅ Status: 200 OK");
            println!("📦 Response: {}", serde_json::to_string_pretty(&response)?);
        }
        Err(e) => {
            println!("❌ Status: 400 Bad Request");
            println!("📦 Error: {e}");
        }
    }

    // Test error handling
    println!("\n📞 GET /accounts?page=0&per_page=5 (invalid)");

    let bad_query = AxumAccountsQuery {
        page: Some(0), // Invalid page
        per_page: Some(5),
        account_type: None,
        sort: None,
        order: None,
    };

    match axum_list_accounts(ledger, bad_query).await {
        Ok(_) => println!("❌ Should have failed!"),
        Err(e) => {
            println!("✅ Status: 400 Bad Request");
            println!("📦 Error: {e}");
        }
    }

    Ok(())
}

/// Actix Web-style request handlers  
async fn actix_style_handlers(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n\n🕸️  Actix Web-Style Handler Examples");
    println!("{}", "=".repeat(40));

    // Simulate Actix web::Query
    println!("\n📞 GET /api/accounts?page=1&limit=10");

    let query = ActixQuery {
        page: 1,
        limit: 10,
        filter: None,
    };

    let response = actix_get_accounts(ledger, query).await?;
    println!("✅ Status: 200 OK");
    println!("📦 Headers: X-Total-Count: {}", response.total);
    println!("📦 Body: {} accounts returned", response.accounts.len());

    // Show first few accounts
    for account in response.accounts.iter().take(3) {
        println!("   • {} - {}", account.code, account.name);
    }

    // Test with filtering
    println!("\n📞 GET /api/accounts?page=1&limit=5&filter=liability");

    let query = ActixQuery {
        page: 1,
        limit: 5,
        filter: Some("liability".to_string()),
    };

    let response = actix_get_accounts(ledger, query).await?;
    println!("✅ Status: 200 OK");
    println!(
        "📦 Filtered results: {} liability accounts",
        response.accounts.len()
    );

    Ok(())
}

/// Warp-style request handlers
async fn warp_style_handlers(
    ledger: &Ledger<MemoryStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n\n🌊 Warp-Style Handler Examples");
    println!("{}", "=".repeat(40));

    // Simulate Warp query parameters
    println!("\n📞 GET /accounts?offset=5&limit=3&type=income");

    let params = WarpQueryParams {
        offset: Some(5),
        limit: Some(3),
        account_type: Some("income".to_string()),
    };

    let response = warp_accounts_handler(ledger, params).await?;

    println!("✅ Status: 200 OK");
    println!("📦 Response Headers:");
    println!("   X-Total-Count: {}", response.meta.total);
    println!("   X-Page-Count: {}", response.meta.page_count);

    println!("📦 Response Body:");
    for account in &response.data {
        println!(
            "   💰 {} - {} (Balance: ${})",
            account.id, account.name, account.current_balance
        );
    }

    Ok(())
}

// Axum-style types and handlers
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AxumAccountsQuery {
    page: Option<u32>,
    per_page: Option<u32>,
    #[serde(rename = "type")]
    account_type: Option<String>,
    sort: Option<String>,
    order: Option<String>,
}

#[derive(Debug, Serialize)]
struct AxumAccountsResponse {
    data: Vec<AxumAccountDto>,
    pagination: AxumPaginationInfo,
}

#[derive(Debug, Serialize)]
struct AxumAccountDto {
    id: String,
    name: String,
    account_type: String,
    balance: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct AxumPaginationInfo {
    page: u32,
    per_page: u32,
    total: u32,
    pages: u32,
    has_next: bool,
    has_prev: bool,
}

async fn axum_list_accounts(
    ledger: &Ledger<MemoryStorage>,
    query: AxumAccountsQuery,
) -> Result<AxumAccountsResponse, String> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);

    let pagination = PaginationParams::new(page, per_page)
        .map_err(|e| format!("Invalid pagination parameters: {e}"))?;

    let result = if let Some(type_str) = query.account_type {
        let account_type =
            parse_account_type(&type_str).map_err(|e| format!("Invalid account type: {e}"))?;
        ledger
            .list_accounts_by_type(account_type, PaginationOption::Paginated(pagination))
            .await
    } else {
        ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await
    }
    .map_err(|e| format!("Database error: {e}"))?;

    let result = result.into_paginated_response();
    let data = result
        .items
        .into_iter()
        .map(|account| AxumAccountDto {
            id: account.id,
            name: account.name,
            account_type: format!("{:?}", account.account_type),
            balance: account.balance.to_string(),
            created_at: account.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        })
        .collect();

    Ok(AxumAccountsResponse {
        data,
        pagination: AxumPaginationInfo {
            page: result.page,
            per_page: result.page_size,
            total: result.total_count,
            pages: result.total_pages,
            has_next: result.has_next,
            has_prev: result.has_previous,
        },
    })
}

// Actix Web-style types and handlers
#[derive(Debug, Deserialize)]
struct ActixQuery {
    page: u32,
    limit: u32,
    filter: Option<String>,
}

#[derive(Debug, Serialize)]
struct ActixAccountsResponse {
    accounts: Vec<ActixAccountDto>,
    page: u32,
    limit: u32,
    total: u32,
    pages: u32,
}

#[derive(Debug, Serialize)]
struct ActixAccountDto {
    code: String,
    name: String,
    type_name: String,
    balance: f64,
}

async fn actix_get_accounts(
    ledger: &Ledger<MemoryStorage>,
    query: ActixQuery,
) -> Result<ActixAccountsResponse, Box<dyn std::error::Error>> {
    let pagination = PaginationParams::new(query.page, query.limit)?;

    let result = if let Some(filter) = query.filter {
        let account_type = parse_account_type(&filter)?;
        ledger
            .list_accounts_by_type(account_type, PaginationOption::Paginated(pagination))
            .await?
    } else {
        ledger
            .list_accounts(PaginationOption::Paginated(pagination))
            .await?
    };
    let result = result.into_paginated_response();

    let accounts = result
        .items
        .into_iter()
        .map(|account| ActixAccountDto {
            code: account.id,
            name: account.name,
            type_name: format!("{:?}", account.account_type),
            balance: account.balance.to_string().parse().unwrap_or(0.0),
        })
        .collect();

    Ok(ActixAccountsResponse {
        accounts,
        page: result.page,
        limit: result.page_size,
        total: result.total_count,
        pages: result.total_pages,
    })
}

// Warp-style types and handlers
#[derive(Debug, Deserialize)]
struct WarpQueryParams {
    offset: Option<u32>,
    limit: Option<u32>,
    #[serde(rename = "type")]
    account_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct WarpAccountsResponse {
    data: Vec<WarpAccountDto>,
    meta: WarpMeta,
}

#[derive(Debug, Serialize)]
struct WarpAccountDto {
    id: String,
    name: String,
    type_code: String,
    current_balance: String,
}

#[derive(Debug, Serialize)]
struct WarpMeta {
    total: u32,
    offset: u32,
    limit: u32,
    page_count: u32,
}

async fn warp_accounts_handler(
    ledger: &Ledger<MemoryStorage>,
    params: WarpQueryParams,
) -> Result<WarpAccountsResponse, Box<dyn std::error::Error>> {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    // Convert offset to page number
    let page = (offset / limit) + 1;
    let pagination = PaginationParams::new(page, limit)?;

    let result = if let Some(type_str) = params.account_type {
        let account_type = parse_account_type(&type_str)?;
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
        .map(|account| WarpAccountDto {
            id: account.id,
            name: account.name,
            type_code: account_type_to_code(&account.account_type),
            current_balance: account.balance.to_string(),
        })
        .collect();

    Ok(WarpAccountsResponse {
        data,
        meta: WarpMeta {
            total: result.total_count,
            offset,
            limit,
            page_count: result.total_pages,
        },
    })
}

// Helper functions
fn parse_account_type(type_str: &str) -> Result<AccountType, Box<dyn std::error::Error>> {
    match type_str.to_lowercase().as_str() {
        "asset" => Ok(AccountType::Asset),
        "liability" => Ok(AccountType::Liability),
        "equity" => Ok(AccountType::Equity),
        "income" => Ok(AccountType::Income),
        "expense" => Ok(AccountType::Expense),
        _ => Err(format!("Unknown account type: {type_str}").into()),
    }
}

fn account_type_to_code(account_type: &AccountType) -> String {
    match account_type {
        AccountType::Asset => "AST".to_string(),
        AccountType::Liability => "LBL".to_string(),
        AccountType::Equity => "EQY".to_string(),
        AccountType::Income => "INC".to_string(),
        AccountType::Expense => "EXP".to_string(),
    }
}
