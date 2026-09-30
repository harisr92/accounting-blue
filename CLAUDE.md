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
| `src/invoice/` | GSTIN, GST invoices, HSN/SAC master data, the pure print model, and the PDF renderer behind the `pdf` feature |
| `src/returns/` | GST returns built from invoices: the pure GSTR-1 builder, its portal-schema DTOs and JSON export, and `ReturnPeriod` |
| `src/reconciliation/` | Pure matching engine, split by pass, with scoring and the report |
| `src/utils/` | `MemoryStorage`, `MemoryReconciliationStorage`, validation rules, Indian amount formatting |
| `src/**/tests/` | Unit test module for its directory: `mod.rs` plus one `<module>_test.rs` per module (see [Tests](#tests)) |
| `tests/` | Integration tests over the public API |

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
| Printing amounts (Indian grouping, amount in words) | `utils::formatting` |

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

### Tests
- Unit tests go in their own files, never inline. Each directory module has a `tests/` folder that is itself a module: a `tests/mod.rs` listing one `<module>_test.rs` file per module in that directory.

  ```text
  src/reconciliation/
  ├── mod.rs              # ends with: #[cfg(test)] mod tests;
  ├── scoring.rs          # no test code at all
  ├── similarity.rs
  └── tests/
      ├── mod.rs          # mod scoring_test; mod similarity_test; ...
      ├── scoring_test.rs
      └── similarity_test.rs
  ```

- The parent `mod.rs` ends with the `tests` module, and nothing else in the source files mentions tests. For the crate root, `lib.rs` plays the role of `mod.rs`:

  ```rust
  #[cfg(test)]
  mod tests;
  ```

- Where tests go:

  | Module | Test file |
  |--------|-----------|
  | `src/reconciliation/scoring.rs` | `src/reconciliation/tests/scoring_test.rs` |
  | `src/tax/gst.rs` | `src/tax/tests/gst_test.rs` |
  | `src/types.rs` (declared in `lib.rs`) | `src/tests/types_test.rs` |
  | `src/ledger/mod.rs` (a directory module) | `src/tests/ledger_test.rs` |
  | `src/reconciliation/engine/mod.rs` (a directory module) | `src/reconciliation/tests/engine_test.rs` |

  A module's tests live in the `tests/` folder of the module that declares it. For a directory module, that is the folder next to its directory, not inside it.
- Test files import what they test explicitly, e.g. `use crate::reconciliation::scoring::*;` plus any types and crates they need. They are no longer children of the code under test, so they can't use `use super::*;`.
- If a test needs a private helper, make it `pub(super)`. That makes it visible to the parent module, which is where the `tests/` folder lives, and to nothing outside it. Don't use `pub(crate)` or `pub` for this. Examples: `scoring::score_amount`, `invoice::types::gstin_checksum`.
- One `<module>_test.rs` per source module. Don't add extra modules such as `display_tests`; group related tests inside that one file instead.
- Tests that exercise several modules through the public API go in the top-level `tests/` directory as integration tests. Examples of public API usage go in doc comments, which run as doctests.
- Name test functions `test_<behaviour>`, describing the behaviour rather than the function under test.
- Behaviour-preserving refactors must not change any existing test assertion's expected values.

## Verification

Run all of these before handing work off. CI runs the same checks.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets -- -D warnings    # without the optional `pdf` feature
cargo test                       # unit, integration and doc tests
cargo test --all-features        # adds the `pdf` renderer's tests
cargo doc --no-deps --all-features
for ex in basic_ledger gst_calculations gst_invoice gstr1_export reconciliation \
          pagination_demo api_pagination_patterns web_integration; do
  cargo run -q --example "$ex" > /dev/null || echo "FAILED: $ex"
done
cargo run -q --features pdf --example gst_invoice_pdf > /dev/null || echo "FAILED: gst_invoice_pdf"
```
