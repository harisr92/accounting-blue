# Pagination Examples

This document provides a comprehensive guide to the pagination examples included with the accounting-core library.

## Available Examples

### 1. `pagination_demo.rs` - Complete Pagination Walkthrough

A comprehensive demonstration of all pagination features including:

- **Basic Account Pagination**: List accounts with different page sizes
- **Filtered Pagination**: Filter by account type while paginating
- **Transaction Pagination**: Paginate transactions with date filtering
- **Account-Specific Transactions**: Get transactions for a specific account
- **Navigation Helpers**: Build pagination controls for user interfaces
- **Error Handling**: Proper validation and error responses
- **UI Helpers**: Practical examples for building pagination UIs

**Run the example:**
```bash
cargo run --example pagination_demo
```

**Key features demonstrated:**
- `PaginationParams::new(page, page_size)` validation
- `PaginatedResponse` metadata usage
- Navigation between pages
- Real-world pagination scenarios

### 2. `api_pagination_patterns.rs` - REST API and GraphQL Patterns

Shows how to integrate pagination with modern API patterns:

- **REST API Patterns**: Traditional query parameter handling
- **GraphQL Pagination**: Cursor-based pagination with edges/nodes
- **Infinite Scroll**: Mobile-friendly batch loading
- **Data Table Pagination**: Server-side table pagination with sorting

**Run the example:**
```bash
cargo run --example api_pagination_patterns
```

**API styles demonstrated:**
- REST: `GET /accounts?page=1&limit=10&type=asset`
- GraphQL: `accounts(first: 5, after: cursor) { edges { node } }`
- Infinite scroll: Progressive loading patterns
- Data tables: Sortable, filterable pagination

### 3. `web_integration.rs` - Web Framework Integration

Demonstrates integration with popular Rust web frameworks:

- **Axum-style**: Query parameters with `serde` deserialization
- **Actix Web-style**: Request handlers with comprehensive error handling
- **Warp-style**: Filter-based request processing

**Run the example:**
```bash
cargo run --example web_integration
```

**Framework patterns:**
- Request parameter validation
- Response formatting for different frameworks
- Error handling best practices
- Header-based metadata (e.g., `X-Total-Count`)

## Common Patterns

### Basic Pagination Setup

```rust
use accounting_core::{Ledger, PaginationParams};

// Create pagination parameters (validates input)
let pagination = PaginationParams::new(1, 20)?; // page 1, 20 items

// Get paginated results
let result = ledger.list_accounts_paginated(pagination).await?;

// Access pagination metadata
println!("Page {} of {}", result.page, result.total_pages);
println!("Showing {} of {} total items", result.items.len(), result.total_count);

// Check navigation availability
if result.has_next {
    // Load next page
}
if result.has_previous {
    // Load previous page  
}
```

### Filtered Pagination

```rust
use accounting_core::{AccountType, PaginationParams};
use chrono::NaiveDate;

// Filter by account type
let pagination = PaginationParams::new(1, 10)?;
let assets = ledger.list_accounts_by_type_paginated(AccountType::Asset, pagination).await?;

// Filter transactions by date range
let start_date = NaiveDate::from_ymd_opt(2024, 1, 1);
let end_date = NaiveDate::from_ymd_opt(2024, 12, 31);
let transactions = ledger.get_transactions_paginated(start_date, end_date, pagination).await?;
```

### REST API Response Format

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize)]
struct ApiResponse<T> {
    data: Vec<T>,
    pagination: PaginationMeta,
}

#[derive(Serialize)]
struct PaginationMeta {
    page: u32,
    per_page: u32,
    total: u32,
    total_pages: u32,
    has_next: bool,
    has_previous: bool,
}
```

### Error Handling

```rust
use accounting_core::PaginationParams;

// Validation errors are caught early
match PaginationParams::new(0, 10) {
    Ok(_) => unreachable!(),
    Err(e) => println!("Invalid page: {}", e), // "Page must be 1 or greater"
}

match PaginationParams::new(1, 1001) {
    Ok(_) => unreachable!(),
    Err(e) => println!("Invalid page size: {}", e), // "Page size must be between 1 and 1000"
}
```

## Running All Examples

To run all examples and see comprehensive pagination functionality:

```bash
# Run each example individually
cargo run --example pagination_demo
cargo run --example api_pagination_patterns  
cargo run --example web_integration

# Or run tests to verify functionality
cargo test
cargo test --doc
```

## Integration Tips

### Database Integration

When implementing the `AccountStore` and `TransactionStore` traits for database backends, push pagination down into the query. (The in-memory store uses `PaginationOption::paginate` instead.)

```rust
// SQL example with LIMIT/OFFSET
async fn list_accounts_paginated(
    &self,
    account_type: Option<AccountType>,
    pagination: PaginationParams,
) -> LedgerResult<PaginatedResponse<Account>> {
    let mut query = "SELECT * FROM accounts".to_string();
    
    if let Some(account_type) = account_type {
        query.push_str(&format!(" WHERE account_type = '{:?}'", account_type));
    }
    
    // Get total count for metadata
    let count_query = query.replace("SELECT *", "SELECT COUNT(*)");
    let total_count: u32 = sqlx::query_scalar(&count_query)
        .fetch_one(&self.pool)
        .await?;
    
    // Add pagination
    query.push_str(&format!(" ORDER BY id LIMIT {} OFFSET {}", 
                           pagination.limit(), 
                           pagination.offset()));
    
    let accounts: Vec<Account> = sqlx::query_as(&query)
        .fetch_all(&self.pool)
        .await?;
        
    Ok(PaginatedResponse::new(
        accounts,
        pagination.page,
        pagination.page_size,
        total_count,
    ))
}
```

### Frontend Integration

The pagination metadata is designed to work seamlessly with frontend frameworks:

```javascript
// React example
function AccountsList({ page, setPage }) {
  const [accounts, setAccounts] = useState(null);
  
  useEffect(() => {
    fetch(`/api/accounts?page=${page}&limit=20`)
      .then(res => res.json())
      .then(data => setAccounts(data));
  }, [page]);
  
  if (!accounts) return <div>Loading...</div>;
  
  return (
    <div>
      {accounts.data.map(account => <div key={account.id}>{account.name}</div>)}
      
      <div className="pagination">
        {accounts.meta.has_previous && (
          <button onClick={() => setPage(page - 1)}>Previous</button>
        )}
        
        <span>Page {accounts.meta.page} of {accounts.meta.total_pages}</span>
        
        {accounts.meta.has_next && (
          <button onClick={() => setPage(page + 1)}>Next</button>
        )}
      </div>
    </div>
  );
}
```

## Performance Considerations

- **Page Size Limits**: The library enforces a maximum page size of 1000 items to prevent memory issues
- **Database Indexes**: Ensure your database has appropriate indexes on commonly filtered/sorted columns
- **Caching**: Consider caching total counts for frequently accessed endpoints
- **Consistent Sorting**: Always use consistent sorting (like ID) to prevent items appearing on multiple pages

## Best Practices

1. **Always validate pagination parameters** before processing requests
2. **Use appropriate page sizes** - typically 10-50 for UI lists, up to 100 for data exports
3. **Include comprehensive metadata** in API responses for building navigation
4. **Handle edge cases** gracefully (empty results, out-of-range pages)
5. **Consider performance** when dealing with large datasets
6. **Maintain consistency** in your API response formats across endpoints