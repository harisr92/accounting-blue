# Changelog

## Unreleased

### Added
- `tax::round_to_paise` and `tax::PAISE_SCALE`: money rounding to the paisa, half-up (0.025 becomes 0.03). This is the one rounding rule in the crate.
- Invoice compliance checks (`accounting_core::invoice::validation`):
  - `validate_invoice(&invoice, as_of, &HsnMaster)` returns an `InvoiceValidationReport` listing every `ComplianceIssue` it finds, each with a `Severity`. Use `is_compliant()`, `errors()` and `warnings()` to read it.
  - Errors: invalid invoice number, no line items, a missing or malformed HSN/SAC code, invalid line fields (empty description, negative price, non-positive quantity, rate outside 0-100), a date after `as_of`, and the same seller and buyer GSTIN.
  - Warnings:
    - an HSN/SAC code the master doesn't know
    - a rate that differs from the default for its exact code. This is not checked when the code only matches a fallback heading, when the line already has a field error, or before `HsnMaster::effective_from`.
    - a line whose taxable value rounds to zero paise, including a zero unit price
  - `validate_invoice` is pure: the caller passes the date to check against and the HSN/SAC master to compare with, usually `HsnMaster::global()`.
- Invoice ledger posting (`accounting_core::invoice::posting`):
  - `GstInvoice::to_entries(&InvoiceAccounts)` posts the invoice as Dr receivable = total, Cr sales = taxable value, and Cr CGST/SGST or IGST output for the tax. The posting balances by construction.
  - `to_entries` refuses an invoice that breaks an error-severity compliance rule and returns the new `PostingError`:
    - It checks the invoice as of its own date, so the future-date rule doesn't apply. Warnings don't block the posting.
    - `NotCompliant` carries the errors, and `NothingToPost` means the invoice total is zero.
  - `posting_legs` maps a `GstBreakdown` onto the `PostingLeg`s and leaves out zero legs.

- Indian amount formatting (`accounting_core::utils::formatting`):
  - `format_inr` rounds to the paisa and groups digits the Indian way: `12,34,567.89`.
  - `amount_in_words` spells an amount out in crore, lakh and thousand, for example "Rupees Twelve Lakh Fifty Thousand and Fifty Paise Only".
- A printable view of an invoice (`accounting_core::invoice::print`):
  - `InvoicePrint::from_invoice(&invoice, &InvoiceParties)` is pure. It returns everything a printed tax invoice shows, with every amount formatted: title, number, date (`dd-mm-yyyy`), place of supply, the parties, one `PrintRow` per line, the CGST/SGST or IGST `TaxLine`s, the totals and the total in words. It serialises to JSON as is.
  - `InvoiceParty` holds a name, address lines and a GSTIN, and `InvoiceParties` pairs a seller with a buyer. `GstInvoice` is unchanged: the parties are passed alongside it. A party with an empty name, or whose GSTIN is not the one on the invoice, is refused with `InvoiceError::InvalidParty { role: PartyRole, reason: PartyError }`.
  - `paginate_rows(&rows, RowCapacity)` splits the item rows into pages. It adds an empty last page when the totals would not fit below the last rows.
- PDF export behind the new `pdf` cargo feature (`accounting_core::invoice::pdf`). The feature pulls in `printpdf` 0.12, which needs Rust 1.88; the crate without the feature still builds on 1.82.
  - `GstInvoice::to_pdf(&InvoiceParties, &PdfOptions)` and `render_pdf(&InvoicePrint, &PdfOptions)` return A4 PDF bytes.
    - The page has a logo placeholder, the seller and the invoice details, the buyer, the item table and the tax summary with the total in words.
    - It ends with any terms, a signature line and a footer note ("E-Invoice Ready" by default) with page numbers.
  - Item tables longer than a page continue on further pages. A description too long for its column is cut short with an ellipsis; a number too wide for its column is set in a smaller size, never cut.
  - `PdfOptions` sets the font, currency label, terms and footer note.
    - `PdfFont::Builtin` (the default) uses standard Helvetica and labels amounts `Rs.`.
    - `PdfFont::Custom { regular, bold }` embeds a TrueType or OpenType font. Use it with a currency label of `₹`, or for names in an Indian script.
  - `PdfError` is the error type, and it converts into `accounting_core::Error::Pdf`. Its variants:
    - `InvalidFont` for font bytes that don't parse.
    - `MissingGlyph` for a character the font can't draw. Every character on the page is checked, including the renderer's own labels.
    - `TermsTooLong` for terms that don't fit below the totals on one page, since the totals, terms and signature always share a page.
    - `Invoice` for an invalid invoice or parties.
  - New example: `cargo run --example gst_invoice_pdf --features pdf [font.ttf]`.
### Fixed
- GST amounts are rounded to paise, so fractions of a paisa no longer reach the ledger. Before, one unit at 0.99 at 5% intra-state gave CGST = SGST = 0.02475, and `to_entries` posted a receivable of 1.0395.
  - `GstCalculation::calculate` rounds the base amount and each of CGST, SGST and IGST with `round_to_paise`.
    - The total tax is the sum of the rounded components, and the total is the base plus the tax.
    - CGST and SGST stay equal on an intra-state rate.
    - Every amount has 2 decimal places, so it displays and serialises as, for example, `90.00` instead of `90`. Zero still prints as `0`, which is how `bigdecimal` formats it. Numeric comparisons are unaffected.
  - Invoices round per line. Each line's breakdown is rounded, and the invoice breakdown is the sum of the rounded lines, as the e-invoice schema expects.
  - `GstCalculation::reverse_calculate` used to carry up to 100 digits. It now rounds the given total to paise and returns exactly that total, with base + tax adding up to it, so the result can be posted against the amount actually received. The tax is calculated forward with CGST = SGST, and the base absorbs the rounding. At every GST slab the base is within a paisa of the exact division.
  - `percent_of` is unchanged and still exact.
- The cash flow statement now classifies a ledger built on `STANDARD_CHART`. Before, `classify_cash_flow` only looked for keywords in account ids, and the standard chart's ids are numbers (`"2100"`), so every transaction was operating. It now reads the accounts a transaction posts to:
  - Financing: an equity account alongside an asset account (so closing entries to retained earnings are not financing), or a liability whose id or name mentions a loan, borrowing, debt, debentures, a mortgage, an overdraft, notes payable or dividends.
  - Investing: a long-lived asset account together with what it is exchanged for, which is another asset such as cash, or a liability for a purchase on credit. An asset is long-lived when its id or name mentions a fixed asset, equipment, machinery, plant, furniture, a vehicle, a building or property. Accumulated depreciation, amortisation and impairment accounts are not long-lived assets.
  - The description is no longer read. Before, an id mentioning `cash` or `asset` plus "equipment" in the description made a transaction investing, so an "Equipment repair" paid from `cash` was investing. Depreciation, repairs and write-downs are now operating.
  - Operating: everything else. Keywords are case-insensitive, and `_` and `-` read as spaces.
  - Trade payables are now operating, as AS 3 / Ind AS 7 treat them. Before, any account id containing `payable` made a transaction financing, so a purchase on `accounts_payable` moves from financing to operating.
  - Cash flow amounts are unchanged: each item is still its transaction's total debits.
- `Ledger::delete_account` no longer deletes an account that is still in use. Before, it removed an account with posted transactions, which left those transactions pointing at nothing: the trial balance and balance sheet lost that side of every entry, and the cash flow statement could no longer tell what the account was. It also removed a parent and left its children pointing at a missing parent. Delete the account's transactions and its child accounts first; the account is left in place when the delete is refused.

### Breaking changes
- `LedgerError` has two new variants, `AccountHasTransactions(String)` and `AccountHasChildren(String)`, returned by `Ledger::delete_account` for an account that is still in use. An exhaustive `match` on `LedgerError` needs arms for them. Code that deleted a used account through the `Ledger` now gets an error; `AccountStore::delete_account` itself is unchanged.
- `reports::classify_cash_flow` and `reports::cash_flow` take the accounts as a new `&HashMap<String, Account>` argument, keyed by account id. `Ledger::generate_cash_flow` is unchanged and passes every account in the ledger.
- `accounting_core::Error` has a new `Posting(PostingError)` variant, and with the `pdf` feature a `Pdf(PdfError)` variant. An exhaustive `match` on it needs arms for them.
- `InvoiceError` has a new `InvalidParty { role, reason }` variant, returned when an invoice is printed with parties that don't match it. An exhaustive `match` on `InvoiceError` needs an arm for it.
- `GstCalculation` and `GstBreakdown` amounts now serialise with 2 decimal places (`"90.00"`, not `"90"`), so a consumer that compares the serialised strings sees different output.
- `GstCalculation::reverse_calculate` rounds a given total with fractions of a paisa to paise, so its `total_amount` is the rounded total.

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
