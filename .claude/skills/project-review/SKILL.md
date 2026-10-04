---
name: project-review
description: Review changed code against this crate's CLAUDE.md coding standards (functional core and imperative shell, SOLID, DRY homes, typed errors, API surface, size, test layout) and its money and GST rules. Use before reporting any code change as done, when asked to review a diff or branch, or after a refactor. Green clippy and tests do not prove these rules hold, so run this pass anyway.
---

# Project review

This skill checks the current change against **Coding standards** and **Verification** in the root `CLAUDE.md`. CLAUDE.md is the source of truth: re-read the section you are checking before every review. This skill doesn't restate the rules. It adds the procedure and the checks most often missed in this crate: money handling, GST rules, the portal JSON, and the test layout.

## 1. Scope the change

```bash
git status --short             # untracked files are part of the change
git diff --stat HEAD
git diff HEAD
```

Read every new file in full, and every changed hunk with the function around it. Review only what changed, plus the code it now calls. If local `main` is behind, compare against `origin/main`, not `main`.

## 2. Walk each new or changed function

For each one, answer the questions below. A "no" is a finding.

**Functional core, imperative shell**
- Is accounting logic pure? This covers balances, reports, GST maths, invoice classification, GSTR-1 building and reconciliation scoring. Pure means plain values or slices in and a value out, with no storage, no `async`, and no clock.
- Does a rule read "today" itself? `Utc::now()` or `Local::now()` in the core is a finding: the caller passes the date, as `validate_invoice(&invoice, as_of, …)` does.
- Is I/O kept to `Ledger`, the ledger operations and the storage impls?
- Are there `mut` accumulators or index loops that could be `map`/`filter`/`fold`/`try_fold`/`sum`? A `for` loop is fine only where it `await`s or is clearer.
- Does the function take ownership where `&T` or `&[T]` would do, or mutate an argument instead of returning a new value? Is there a needless `.clone()`?

**SOLID**
- **Open/closed:** does a new case grow a long `match` in an unrelated function? Add a variant, a rule function (like the validation rules) or a section function (like `b2cl_section`/`b2cs_section`) instead.
- **Single responsibility:** does a function both decide and print, or both compute and persist? Do storage traits in `traits.rs` stay data-access only, never computing balances or reports?
- **Interface segregation:** is a new trait wider than its one consumer needs? Does a function take a whole `GstInvoice` or `Ledger` when it reads two fields?
- **Dependency inversion:** does core code name `MemoryStorage` or another concrete store instead of the traits in `traits.rs`?

**DRY: one home per rule**
Before accepting a new helper, search `src/` for an existing one. A copy of any of these is a finding:

| Rule | Home |
|------|------|
| Debit/credit sign | `AccountType::balance_effect`, `EntryType::signed` (`src/types.rs`) |
| Pagination | `PaginationOption::paginate`; print pages: `invoice::print::paginate_rows` |
| GST rate by supply type | `GstRate::for_supply` (`src/tax/gst.rs`) |
| Rounding money to paise | `tax::round_to_paise`, the only rounding rule |
| Indian amounts and words | `utils::formatting::{format_inr, amount_in_words}` |
| Validation rules | `utils::validation`; invoice compliance: `invoice::validation::compliance_errors` |
| B2B / B2CL / B2CS classification | `GstInvoice::supply_kind`, `supply_kind_for`, `b2cl_threshold` |
| Portal JSON numbers and dates | `portal_amount`, `portal_number`, `portal_date` in `returns/gstr1.rs` |

**Money and GST**
- Is money `BigDecimal` all the way through? An `f64` is allowed only when serialising to the portal's JSON numbers (`portal_number`) and in PDF layout coordinates.
- Is every rounding done through `round_to_paise`? Are line amounts rounded before they are summed, as `GstCalculation` does?
- Are thresholds, rates and dates named `const`s with a doc comment citing the rule? Examples: `B2CL_THRESHOLD_RUPEES` (Notification 12/2024-CT) and `UNREGISTERED_DETAILS_THRESHOLD_RUPEES` (Rule 46).
- If a rule changed on a date, is the old value kept and chosen by the invoice's date, as `b2cl_threshold` does?
- Is the comparison right (`>` vs `>=`) for what the rule says? Is it on the right value (taxable value vs invoice value including tax)?
- Does the tax split come from the seller's state and the place of supply (`is_inter_state`), never from the buyer's GSTIN directly?
- Are amounts in a new GSTR-1 section filed in the right table? Watch for nil-rated or exempt lines landing in a taxable table.

**Types and serde**
- Can an invalid value exist? Constructors validate, and `Deserialize` goes through the same path: `#[serde(try_from = "Raw…")]` or `try_from = "String"`, as `GstInvoice`, `GstLineItem`, `Gstin` and `StateCode` do.
- Do portal DTOs use the portal's short keys (`#[serde(rename = …)]`) with the right `serialize_with`? Are keys that must be absent skipped (`skip_serializing_if`), not written as `0` or `null`?

**Errors**
- Are errors typed `thiserror` enums with structured variants, and is there no new catch-all `String` variant for a failure a caller might want to tell apart?
- Does every module error reach `crate::Error` through `#[from]`?
- Is there an `unwrap`, `expect`, `panic!` or unchecked index (`v[i]`, `&s[a..b]` on untrusted input) in library code? The only exception is data embedded at compile time, which needs `#[allow(clippy::expect_used)]`, a reason and a test.
- Is there an error variant that nothing raises any more? Remove it if it is still unreleased in `CHANGELOG.md`.

**API surface**
- Is every public item documented, with `# Errors` on fallible functions and an example where it helps?
- Are re-exports explicit by name in the module's `mod.rs`, and in `lib.rs` where siblings are re-exported? No glob `pub use`.
- Are string parameters `impl Into<String>`? Is the naming right: `into_*` consumes, `as_*`/`to_*` borrows? Do getters return `&[T]`, not `&Vec<T>`?
- Is every visible change listed in `CHANGELOG.md` under Unreleased? Look for new fields on public structs, new enum variants, changed signatures and changed JSON shapes. Breaking ones go under **Breaking changes**.
- Do `README.md` and the module docs still describe what the code does? Watch for "not covered yet" lists that went stale.

**Size and style**
- Is any library function over 60 lines (`clippy.toml`)? Split it by responsibility, not arbitrarily.
- Are magic numbers named `const`s, or put in config (`ReconciliationConfig`, `ScoringWeights`)?

## 3. Tests

- Does every new pure function and rule have a unit test, with the boundary values the rule names (exactly at a threshold, one paisa over)?
- Do unit tests live in `<dir>/tests/<module>_test.rs`, listed in `<dir>/tests/mod.rs`? There must be one file per source module, and no extra `*_tests` modules.
- Do test files import explicitly (`use crate::…::*;`), not `use super::*;`?
- Are helpers used only by tests `pub(super)`, never `pub(crate)` or `pub`?
- Are test names `test_<behaviour>`?
- Is a portal JSON shape checked with an exact `json!` literal, not only `contains`?
- Do cross-module flows have an integration test in `tests/`?
- Did any existing assertion's expected value change? That is allowed only when the change is meant to alter that behaviour, and the change must say so.
- No inline tests in source files. This should print nothing:

  ```bash
  git diff HEAD --name-only -- 'src/**.rs' | grep -v '/tests/' | xargs grep -ln '#\[test\]'
  ```

## 4. Fix, then verify

- Fix every finding before reporting the work done. If a fix would be a large redesign or a judgement call, list it and ask instead.
- Then run the whole **Verification** block from CLAUDE.md, not a subset:
  - fmt
  - clippy with and without `--all-features`
  - `cargo test` with and without `--all-features`
  - `cargo doc`
  - every example, including `gst_invoice_pdf` with `--features pdf`

## 5. Report

List each finding as `file:line`, the rule broken, and what you changed. Leave out rules that passed. Say plainly if any check could not run, and why.
