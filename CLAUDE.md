# accounting-core

Rust library for double-entry bookkeeping, Indian GST (tax maths and B2B invoices), financial reports and bank/gateway reconciliation. Money is `bigdecimal::BigDecimal` everywhere. Storage is pluggable behind async traits, and `MemoryStorage` is the reference implementation.

## Layout

| Path | Responsibility |
|------|----------------|
| `src/error.rs` | Crate-level `Error`/`Result` and the ledger's `LedgerError` |
| `src/types.rs` | Domain values: `Account`, `Entry`, `Transaction`, the sign rules, pagination |
| `src/traits.rs` | Ports: `AccountStore`, `TransactionStore` (data access only), and the validator traits |
| `src/ledger/` | `Ledger` orchestrator (the imperative shell), account and transaction operations, pure `balances` |
| `src/reports/` | Pure report builders (balance sheet, income statement, cash flow, integrity) and their DTOs |
| `src/tax/gst.rs` | GST rates and calculations |
| `src/invoice/` | GSTIN, GST invoices, HSN/SAC master data |
| `src/reconciliation/` | Pure matching engine, split by pass, with scoring and the report |
| `src/utils/` | `MemoryStorage`, `MemoryReconciliationStorage`, validation rules |

## Coding standards

### Functional core, imperative shell
- Put accounting rules in pure functions over plain data: balances, reports, GST maths, reconciliation scoring. They take values or slices and return values, with no I/O, no storage and no `async`.
- Only the shell does I/O: `Ledger`, the ledger operations and the storage impls load data, call the pure core, then persist the result.
- Prefer iterator chains (`map`, `filter`, `fold`, `try_fold`, `sum`) to `mut` accumulators and index loops. Keep a `for` loop only where it is clearer, for example when it `await`s inside.
- Return new values rather than mutating arguments. Take `&T` or `&[T]` when the function doesn't need ownership.

### SOLID
- **Single responsibility.** A module or type has one reason to change. Storage stores, the core computes, the `Ledger` orchestrates.
- **Open/closed.** Add behaviour by adding a variant, a rule function or a pass. Don't grow a long `match` in an unrelated function.
- **Interface segregation.** Traits stay small. Storage traits do data access only, and never compute balances or reports. `LedgerStorage` is only the blanket combination of `AccountStore + TransactionStore`.
- **Dependency inversion.** The core depends on the traits in `traits.rs`, never on `MemoryStorage`.

### DRY: one home per rule

| Rule | Home |
|------|------|
| Debit/credit sign rule | `AccountType::balance_effect` and `EntryType::signed` |
| Pagination | `PaginationOption::paginate` |
| Choosing a GST rate by supply type | `GstRate::for_supply` |
| Validation rules | `utils::validation` |

If you are about to copy a block, extract it into its home instead.

### Errors
- Use typed `thiserror` enums with structured variants. Don't add a new catch-all `String` variant for failures a caller might want to tell apart.
- Every module error converts into `crate::Error` through `#[from]`.
- Library code must not use `unwrap`, `expect`, `panic!` or unchecked indexing on untrusted input. The one exception is data embedded at compile time: it needs `#[allow(clippy::expect_used)]` with a reason and a test that forces the load.
- Map lock poisoning and backend failures to a `Storage` error that carries its source.

### API surface
- Re-export explicitly by name. No glob `pub use`.
- Take string parameters as `impl Into<String>`.
- Use `into_*` for consuming conversions and `as_*`/`to_*` for borrowing ones. Getters return slices (`&[T]`), not `&Vec<T>`.
- Every public item has a doc comment. Breaking changes are listed in `CHANGELOG.md`.

### Size and style
- Library functions stay at 60 lines or fewer, enforced by `clippy::too_many_lines` with the threshold in `clippy.toml`. Split by responsibility, not arbitrarily.
- Name magic numbers as `const`s, or put them in config (for example `ReconciliationConfig`, `ScoringWeights`).
- Unit tests live next to the code in a `#[cfg(test)] mod tests`. Cross-module flows go in `tests/`.
- Behaviour-preserving refactors must not change any existing test assertion's expected values.

## Verification

Run all of these before handing work off. CI runs the same checks.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test                       # unit, integration and doc tests
cargo doc --no-deps --all-features
for ex in basic_ledger gst_calculations gst_invoice reconciliation \
          pagination_demo api_pagination_patterns web_integration; do
  cargo run -q --example "$ex" > /dev/null || echo "FAILED: $ex"
done
```
