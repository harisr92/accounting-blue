# Changelog

## 0.2.0

This release refactors the crate to the coding standards in [CLAUDE.md](CLAUDE.md): a functional core behind an imperative shell, small traits, one home per accounting rule, typed errors, and no panics in library code. Balances, GST amounts and reconciliation scores are unchanged, and every existing test assertion passes with the same expected values. Report totals change only where the old ones were wrong: see the balance sheet fix under Fixes. The public API has breaking changes, listed below.

### Breaking changes

#### Errors
- `LedgerError` moved to `accounting_core::error` (it is still re-exported at the crate root). Its catch-all `Validation(String)` and `InvalidTransaction(String)` variants are replaced by structured variants:
  - `DuplicateAccount`
  - `ParentNotFound`
  - `AccountCycle`
  - `TooFewEntries`
  - `Unbalanced { debits, credits }`
  - `NonPositiveAmount`
  - `DuplicateEntry`
  - `InvalidField { field, error: FieldError }`
  - `InvalidPagination(PaginationError)`
- `LedgerError::Storage` and `ReconciliationError::Storage` now carry a boxed source error. Build them with `LedgerError::storage(err)`.
- `GstError::InvalidRate(String)` is split into `ComponentsMismatch`, `UnequalSplit` and `MixedIgstAndSplit`. The unused `GstError::Calculation` is removed.
- `InvoiceError` changes:
  - `InvalidGstin` becomes `{ value, reason: GstinError }`.
  - `InvalidInvoiceNumber` becomes `{ value, reason: InvoiceNumberError }`.
  - `InvalidLineItem` carries a `LineItemError`.
  - An invoice with no lines now fails with `EmptyInvoice`.
- New crate-level `accounting_core::Error` and `accounting_core::Result`. Every module error converts into them via `From`.

#### Storage and ledger
- `LedgerStorage` is split into `AccountStore` and `TransactionStore`, which do data access only. `LedgerStorage` is now a blanket trait over the two.
  - `get_account_transactions` and `get_transactions` are replaced by `list_transactions(&TransactionFilter, PaginationOption)`.
  - `get_account_balance`, `get_trial_balance` and `get_account_balances_by_type` are removed from storage. The ledger computes them with the pure functions in `ledger::balances`.
- `Ledger<S>` owns a single store and no longer requires `S: Clone`. Use `Ledger::storage()` and `Ledger::into_storage()` to reach it.
- `MemoryStorage` and `MemoryReconciliationStorage` are plain owned maps with no locks.
  - Their `clear` and `add_ledger_transaction` methods take `&mut self`.
  - `reconciled_marks` returns a reference.
- Removed:
  - `AccountManager`, `TransactionManager`, `StandardChartOfAccounts`, and the `ChartOfAccounts` and `ReportGenerator` traits. Use the `Ledger` methods `child_accounts`, `account_path` and `setup_standard_chart_of_accounts`. The chart itself is now the public `STANDARD_CHART` table.
  - `ledger::account::utils::create_standard_chart` and the `accounting_core::core` module re-export.
  - The `ZERO` static. Use `BigDecimal::zero()` and `.is_zero()`.
  - `validate_positive_amount` (`Transaction::validate` already enforces it), and the no-op `validate_account_deletion` and `validate_account_references` validator methods.
- `DefaultAccountValidator` and `DefaultTransactionValidator` moved to `utils::validation`, next to the enhanced validators. Both are still re-exported at the crate root.
- Renamed:

  | Old | New |
  |-----|-----|
  | `Ledger::get_transactions` | `list_transactions` |
  | `Ledger::get_account_transactions` | `list_account_transactions` |
  | `Ledger::get_all_transactions` | `list_all_transactions` |
  | `Ledger::get_all_account_transactions` | `list_all_account_transactions` |
  | `ListResponse::to_paginated_response` | `into_paginated_response` |

- `ListResponse::items` returns `&[T]`.
- `reports::total` takes the section's `AccountType` and nets balances on the opposite side instead of adding them.
- Report DTOs (`BalanceSheet`, `IncomeStatement`, `CashFlowStatement`, `CashFlowItem`, `LedgerIntegrityReport`) moved to `accounting_core::reports`. They are still re-exported at the crate root.

#### GST and invoices
- `GstCalculator::calculate_with_rate` is removed. Use `GstCalculation::calculate`.
- `GstCategory::rate`, `intra_state_rate` and `inter_state_rate` take `self` by value.

#### Reconciliation
- `ReconciliationEngine::reconcile` takes `&[LedgerTransaction]` and `&[ExternalTransaction]` instead of owned vectors.
- `ReconciliationConfig` has a new `weights: ScoringWeights` field. The defaults equal the old hard-coded weights.

#### API surface
- Constructors take `impl Into<String>`. This covers `Account`, `Entry`, `Transaction`, `TransactionBuilder`, `Ledger::create_account`, the transaction patterns, `GstLineItem`, `GstInvoice`, `LedgerTransaction` and `ExternalTransaction`. Calls that passed `"x".into()` must now pass `"x"`.
- `EntryType` is `Copy`.
- The crate root, `tax`, `invoice`, `reconciliation` and `utils` re-export items by name instead of by glob.

### Fixes
- `update_transaction` checks that every account on the new version exists before changing any balance. Previously a missing account was skipped silently, which applied only part of the transaction. Reversals (the old version on update, and `delete_transaction`) still skip accounts that have since been deleted, so such a transaction can always be removed.
- Balance sheet and income statement totals now respect the debit/credit side. A net loss, or any balance on the opposite side of its account's normal balance (e.g. an overdrawn asset), used to be added to its section's total instead of subtracted, so a correct ledger with a loss was reported as unbalanced.
- GSTIN parsing:
  - A state code with a sign or other non-digit (e.g. `+7`) is rejected; it used to parse as a number.
  - The checksum no longer maps unknown characters to zero.
- `PaginatedResponse::new` no longer divides by zero when `page_size` is 0, and `PaginationParams::offset` saturates instead of overflowing.
- `account_path` detects cycles in parent links, and builds the path in linear time.

### Internal
- New modules:
  - `error`
  - `reports`, with pure report builders
  - `ledger::balances`, with pure balance and trial-balance functions
  - `reconciliation::scoring`, with per-dimension scoring
- The reconciliation engine is split into `engine::{state, passes::{reference, exact, scored, suggestions}}` and `reconciliation::report`.
- One home per rule:
  - `AccountType::balance_effect`
  - `EntryType::{opposite, signed}`
  - `PaginationOption::paginate`
  - `TransactionFilter`
  - `GstRate::for_supply`
  - `tax::percent_of`
- Lint policy:
  - `unsafe_code` is forbidden.
  - `unwrap`, `expect` and `panic!` are denied in library code.
  - Library functions are capped at 60 lines by `clippy::too_many_lines`.
  - `missing_docs` warns.
  - Pedantic clippy warnings on the library went from 193 to 0.
- `rustfmt.toml` now matches the crate's 2021 edition. CI runs every example.

### Deferred to follow-up work
These are behaviour changes, so this refactor deliberately left them out:
- Round GST amounts to paise, with an explicit rounding mode and a choice between per-line and per-invoice rounding.
- Move `GstCategory` rates to the GST 2.0 schedule (0/5/18/40, effective 22 Sep 2025), driven by the same versioned data as `HsnMaster`.
- Make `record_transaction`, `update_transaction` and `delete_transaction` atomic. They write the transaction and each account separately, so a storage failure part-way leaves partial state.
- Keep `delete_account` from deleting an account that transactions still reference.
- Honour `start_date` in the income statement (it is currently cumulative to `end_date`), and classify cash flow by account type rather than keywords in account ids.
- Inject a clock instead of calling `Utc::now()` inside domain types.
