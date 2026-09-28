//! Turning matched items into a [`ReconciliationReport`]

use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use chrono::NaiveDate;

use crate::reconciliation::types::{
    ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationReport,
    ReconciliationStatus, ReconciliationSummary,
};
use crate::types::EntryType;

/// How many items of each kind a run produced
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    matched: usize,
    unmatched_ledger: usize,
    unmatched_external: usize,
    partial: usize,
}

impl Counts {
    fn tally(items: &[ReconciliationStatus]) -> Self {
        items.iter().fold(Self::default(), |mut counts, item| {
            match item {
                ReconciliationStatus::Matched { .. } => counts.matched += 1,
                ReconciliationStatus::UnmatchedLedger { .. } => counts.unmatched_ledger += 1,
                ReconciliationStatus::UnmatchedExternal { .. } => counts.unmatched_external += 1,
                ReconciliationStatus::PartialMatch { .. } => counts.partial += 1,
            }
            counts
        })
    }
}

/// Tally the items and wrap everything up into a report
pub(crate) fn build_report(
    ledger: &[LedgerTransaction],
    external: &[ExternalTransaction],
    external_source: ExternalSource,
    items: Vec<ReconciliationStatus>,
) -> ReconciliationReport {
    let counts = Counts::tally(&items);
    let (period_start, period_end) = period_bounds(ledger, external);

    let mut account_ids: Vec<String> = ledger.iter().map(|t| t.account_id.clone()).collect();
    account_ids.sort();
    account_ids.dedup();

    ReconciliationReport {
        id: uuid::Uuid::new_v4(),
        created_at: chrono::Utc::now().naive_utc(),
        period_start,
        period_end,
        external_source,
        account_ids,
        total_ledger_transactions: ledger.len(),
        total_external_transactions: external.len(),
        matched_count: counts.matched,
        unmatched_ledger_count: counts.unmatched_ledger,
        unmatched_external_count: counts.unmatched_external,
        partial_match_count: counts.partial,
        reconciliation_items: items,
        summary: summarize(ledger, external, counts.matched),
    }
}

/// Net movement across a set of transactions, counting credits positive
fn net_balance<'a>(movements: impl Iterator<Item = (EntryType, &'a BigDecimal)>) -> BigDecimal {
    movements
        .map(|(entry_type, amount)| entry_type.opposite().signed(amount))
        .sum()
}

/// Aggregate balances, match rate and overall confidence
///
/// Confidence is the plain average of the match rate and the balance agreement.
#[allow(clippy::cast_precision_loss)] // transaction counts are far below 2^52
fn summarize(
    ledger: &[LedgerTransaction],
    external: &[ExternalTransaction],
    matched_count: usize,
) -> ReconciliationSummary {
    let ledger_balance = net_balance(ledger.iter().map(|t| (t.entry_type, &t.amount)));
    let external_balance = net_balance(external.iter().map(|t| (t.entry_type, &t.amount)));
    let difference = &ledger_balance - &external_balance;

    let total = ledger.len().max(external.len());
    let match_rate = if total > 0 {
        matched_count as f64 / total as f64
    } else {
        0.0
    };

    let scale = ledger_balance.abs().max(external_balance.abs());
    let balance_accuracy = if scale.is_zero() {
        1.0
    } else {
        // A ratio too large for f64 counts as no agreement at all
        let ratio = (difference.abs() / &scale).to_f64().unwrap_or(1.0);
        (1.0 - ratio).clamp(0.0, 1.0)
    };

    ReconciliationSummary {
        ledger_balance,
        external_balance,
        difference,
        match_rate,
        confidence_score: (match_rate + balance_accuracy) / 2.0,
    }
}

/// Earliest and latest date across both sides, falling back to today when both are empty
fn period_bounds(
    ledger: &[LedgerTransaction],
    external: &[ExternalTransaction],
) -> (NaiveDate, NaiveDate) {
    ledger
        .iter()
        .map(|t| t.date)
        .chain(external.iter().map(|t| t.date))
        .fold(None, |bounds: Option<(NaiveDate, NaiveDate)>, date| {
            Some(bounds.map_or((date, date), |(start, end)| {
                (start.min(date), end.max(date))
            }))
        })
        .unwrap_or_else(|| {
            let today = chrono::Utc::now().date_naive();
            (today, today)
        })
}
