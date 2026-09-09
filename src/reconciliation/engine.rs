//! The matching engine
//!
//! [`ReconciliationEngine::reconcile`] is pure and synchronous: it takes both sides in memory and
//! returns a [`ReconciliationReport`]. Loading transactions and persisting the report are the
//! caller's job, via [`ReconciliationStorage`](crate::reconciliation::ReconciliationStorage).

use std::collections::{HashMap, VecDeque};

use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDate;

use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::similarity;
use crate::reconciliation::types::*;
use crate::types::EntryType;

/// Weight of the amount dimension when scoring a pair
const AMOUNT_WEIGHT: f64 = 0.35;
/// Weight of the date dimension when scoring a pair
const DATE_WEIGHT: f64 = 0.30;
/// Weight of the description dimension when scoring a pair
const DESCRIPTION_WEIGHT: f64 = 0.15;
/// Weight of the direction dimension when scoring a pair
const ENTRY_TYPE_WEIGHT: f64 = 0.10;
/// Weight of the reference dimension, counted only when both sides carry one
const REFERENCE_WEIGHT: f64 = 0.10;

/// Both sides of a reconciliation, plus the id-sorted traversal orders.
///
/// Every pass walks these orders rather than the vectors directly, so a given pair of inputs
/// always produces the same report however they happened to be sorted on the way in.
struct Sides<'a> {
    ledger: &'a [LedgerTransaction],
    external: &'a [ExternalTransaction],
    ledger_order: Vec<usize>,
    external_order: Vec<usize>,
}

impl<'a> Sides<'a> {
    fn new(ledger: &'a [LedgerTransaction], external: &'a [ExternalTransaction]) -> Self {
        let mut ledger_order: Vec<usize> = (0..ledger.len()).collect();
        ledger_order.sort_by(|&a, &b| ledger[a].id.cmp(&ledger[b].id));

        let mut external_order: Vec<usize> = (0..external.len()).collect();
        external_order.sort_by(|&a, &b| external[a].id.cmp(&external[b].id));

        Self {
            ledger,
            external,
            ledger_order,
            external_order,
        }
    }
}

/// What has been claimed so far, and the report items produced along the way
struct MatchState {
    ledger_taken: Vec<bool>,
    external_taken: Vec<bool>,
    items: Vec<ReconciliationStatus>,
}

impl MatchState {
    fn new(ledger_len: usize, external_len: usize) -> Self {
        Self {
            ledger_taken: vec![false; ledger_len],
            external_taken: vec![false; external_len],
            items: Vec::new(),
        }
    }

    fn is_free(&self, ledger_index: usize, external_index: usize) -> bool {
        !self.ledger_taken[ledger_index] && !self.external_taken[external_index]
    }

    fn claim(&mut self, ledger_index: usize, external_index: usize) {
        self.ledger_taken[ledger_index] = true;
        self.external_taken[external_index] = true;
    }
}

/// A scored pairing considered during the fuzzy pass
struct Candidate {
    ledger_index: usize,
    external_index: usize,
    score: f64,
    differences: Vec<MatchDifference>,
}

/// Matches internal ledger transactions against external ones
///
/// ```
/// use accounting_core::reconciliation::{
///     ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationEngine,
/// };
/// use accounting_core::EntryType;
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
/// let source = ExternalSource::BankStatement {
///     bank_name: "SBI".to_string(),
///     account_number: "12345678901".to_string(),
/// };
///
/// let ledger = vec![LedgerTransaction::new(
///     "txn-1".to_string(),
///     date,
///     BigDecimal::from(1000),
///     "Payment from customer".to_string(),
///     EntryType::Debit,
///     "bank".to_string(),
/// )];
/// let external = vec![ExternalTransaction::new(
///     "ext-1".to_string(),
///     date,
///     BigDecimal::from(1000),
///     "Payment from customer".to_string(),
///     EntryType::Debit,
///     source.clone(),
/// )];
///
/// let report = ReconciliationEngine::default().reconcile(ledger, external, source);
/// assert_eq!(report.matched_count, 1);
/// assert!(report.is_fully_reconciled());
/// ```
#[derive(Debug, Clone, Default)]
pub struct ReconciliationEngine {
    config: ReconciliationConfig,
}

impl ReconciliationEngine {
    /// Create an engine with the given configuration
    pub fn new(config: ReconciliationConfig) -> Self {
        Self { config }
    }

    /// The configuration in use
    pub fn config(&self) -> &ReconciliationConfig {
        &self.config
    }

    /// Match both sides and produce a report.
    ///
    /// Matching runs in four passes: reference numbers, then exact date/amount/direction, then a
    /// scored sweep of what remains assigned best-first, and finally suggestions for whatever is
    /// still unmatched. The result does not depend on the order of either input vector.
    pub fn reconcile(
        &self,
        ledger: Vec<LedgerTransaction>,
        external: Vec<ExternalTransaction>,
        external_source: ExternalSource,
    ) -> ReconciliationReport {
        let items = {
            let sides = Sides::new(&ledger, &external);
            let mut state = MatchState::new(ledger.len(), external.len());

            self.match_by_reference(&sides, &mut state);
            self.match_exactly(&sides, &mut state);

            let candidates = self.score_remaining(&sides, &state);
            self.assign_best_first(&sides, &candidates, &mut state);
            self.collect_unmatched(&sides, &candidates, &mut state);

            state.items
        };

        self.build_report(ledger, external, external_source, items)
    }

    /// Pass 1: a shared reference number, backed by an equal amount and direction, is conclusive.
    ///
    /// A reference is only acted on when it identifies exactly one available counterpart; a
    /// reference reused across several statement rows is left for the scored passes. Dates are
    /// deliberately ignored here, so settlement lag on a row carrying a UTR still matches outright.
    fn match_by_reference(&self, sides: &Sides, state: &mut MatchState) {
        let mut by_reference: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, transaction) in sides.external.iter().enumerate() {
            if let Some(key) = normalized_reference(transaction.reference.as_deref()) {
                by_reference.entry(key).or_default().push(index);
            }
        }

        for &ledger_index in &sides.ledger_order {
            let ledger_transaction = &sides.ledger[ledger_index];
            let Some(key) = normalized_reference(ledger_transaction.reference.as_deref()) else {
                continue;
            };
            let Some(bucket) = by_reference.get(&key) else {
                continue;
            };

            let mut available = bucket.iter().copied().filter(|&index| {
                !state.external_taken[index]
                    && sides.external[index].amount == ledger_transaction.amount
                    && sides.external[index].entry_type == ledger_transaction.entry_type
            });

            let Some(external_index) = available.next() else {
                continue;
            };
            if available.next().is_some() {
                continue; // Ambiguous - let the scored passes decide.
            }

            state.claim(ledger_index, external_index);
            state.items.push(ReconciliationStatus::Matched {
                ledger_id: ledger_transaction.id.clone(),
                external_id: sides.external[external_index].id.clone(),
                match_score: 1.0,
            });
        }
    }

    /// Pass 2: identical date, amount and direction, found by index rather than a nested scan
    fn match_exactly(&self, sides: &Sides, state: &mut MatchState) {
        let mut by_key: HashMap<ExactKey, VecDeque<usize>> = HashMap::new();
        for &external_index in &sides.external_order {
            if state.external_taken[external_index] {
                continue;
            }
            let transaction = &sides.external[external_index];
            by_key
                .entry(exact_key(
                    transaction.date,
                    &transaction.amount,
                    &transaction.entry_type,
                ))
                .or_default()
                .push_back(external_index);
        }

        for &ledger_index in &sides.ledger_order {
            if state.ledger_taken[ledger_index] {
                continue;
            }
            let ledger_transaction = &sides.ledger[ledger_index];
            let Some(bucket) = by_key.get_mut(&exact_key(
                ledger_transaction.date,
                &ledger_transaction.amount,
                &ledger_transaction.entry_type,
            )) else {
                continue;
            };
            let Some(external_index) = bucket.pop_front() else {
                continue;
            };

            state.claim(ledger_index, external_index);
            state.items.push(ReconciliationStatus::Matched {
                ledger_id: ledger_transaction.id.clone(),
                external_id: sides.external[external_index].id.clone(),
                match_score: 1.0,
            });
        }
    }

    /// Pass 3: score every still-plausible pair once, feeding both the assignment and the
    /// suggestion lists. Pairs dated further apart than `candidate_window_days` are not considered.
    fn score_remaining(&self, sides: &Sides, state: &MatchState) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        for &ledger_index in &sides.ledger_order {
            if state.ledger_taken[ledger_index] {
                continue;
            }
            for &external_index in &sides.external_order {
                if state.external_taken[external_index] {
                    continue;
                }
                let days_apart = (sides.ledger[ledger_index].date
                    - sides.external[external_index].date)
                    .num_days()
                    .abs();
                if days_apart > self.config.candidate_window_days {
                    continue;
                }

                let (score, differences) = self.calculate_match_score(
                    &sides.ledger[ledger_index],
                    &sides.external[external_index],
                );
                if score >= self.config.suggestion_threshold {
                    candidates.push(Candidate {
                        ledger_index,
                        external_index,
                        score,
                        differences,
                    });
                }
            }
        }

        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| {
                    sides.ledger[a.ledger_index]
                        .id
                        .cmp(&sides.ledger[b.ledger_index].id)
                })
                .then_with(|| {
                    sides.external[a.external_index]
                        .id
                        .cmp(&sides.external[b.external_index].id)
                })
        });

        candidates
    }

    /// Pass 4: walk the scored pairs best-first, taking each whose both sides are still free.
    ///
    /// Assigning globally rather than per ledger row stops a mediocre pairing from claiming a
    /// counterpart that a stronger pairing later in the input wanted. A pair that disagrees on
    /// money or direction is always reported as a partial match, however high it scores - see
    /// [`requires_review`].
    fn assign_best_first(&self, sides: &Sides, candidates: &[Candidate], state: &mut MatchState) {
        for candidate in candidates {
            if candidate.score < self.config.partial_match_threshold {
                break; // Sorted descending, so nothing further can qualify.
            }
            if !state.is_free(candidate.ledger_index, candidate.external_index) {
                continue;
            }

            state.claim(candidate.ledger_index, candidate.external_index);

            let ledger_id = sides.ledger[candidate.ledger_index].id.clone();
            let external_id = sides.external[candidate.external_index].id.clone();
            let needs_review = requires_review(&candidate.differences);

            if candidate.score >= self.config.auto_match_threshold && !needs_review {
                state.items.push(ReconciliationStatus::Matched {
                    ledger_id,
                    external_id,
                    match_score: candidate.score,
                });
            } else {
                state.items.push(ReconciliationStatus::PartialMatch {
                    ledger_id,
                    external_id,
                    match_score: candidate.score,
                    differences: candidate.differences.clone(),
                    auto_resolvable: candidate.score >= self.config.auto_resolve_threshold
                        && !needs_review,
                });
            }
        }
    }

    /// Pass 5: report what is left, each with its best still-available counterparts
    fn collect_unmatched(&self, sides: &Sides, candidates: &[Candidate], state: &mut MatchState) {
        let mut ledger_suggestions: Vec<Vec<PartialMatch>> = vec![Vec::new(); sides.ledger.len()];
        let mut external_suggestions: Vec<Vec<PartialMatch>> =
            vec![Vec::new(); sides.external.len()];

        // `candidates` is already sorted by score descending, so each list comes out sorted too.
        for candidate in candidates {
            if !state.is_free(candidate.ledger_index, candidate.external_index) {
                continue; // Never suggest a counterpart that is already spoken for.
            }

            let ledger_list = &mut ledger_suggestions[candidate.ledger_index];
            if ledger_list.len() < self.config.max_suggestions {
                ledger_list.push(PartialMatch {
                    counterpart_id: sides.external[candidate.external_index].id.clone(),
                    match_score: candidate.score,
                    differences: candidate.differences.clone(),
                });
            }

            let external_list = &mut external_suggestions[candidate.external_index];
            if external_list.len() < self.config.max_suggestions {
                external_list.push(PartialMatch {
                    counterpart_id: sides.ledger[candidate.ledger_index].id.clone(),
                    match_score: candidate.score,
                    differences: candidate.differences.clone(),
                });
            }
        }

        for &index in &sides.ledger_order {
            if !state.ledger_taken[index] {
                state.items.push(ReconciliationStatus::UnmatchedLedger {
                    ledger_id: sides.ledger[index].id.clone(),
                    possible_matches: std::mem::take(&mut ledger_suggestions[index]),
                });
            }
        }

        for &index in &sides.external_order {
            if !state.external_taken[index] {
                state.items.push(ReconciliationStatus::UnmatchedExternal {
                    external_id: sides.external[index].id.clone(),
                    possible_matches: std::mem::take(&mut external_suggestions[index]),
                });
            }
        }
    }

    /// Score how alike two transactions are, in `0.0..=1.0`, along with every way they disagree.
    ///
    /// Each dimension contributes its weight scaled by how well it agrees. The reference dimension
    /// is dropped from the total when either side lacks a reference, so a missing reference is
    /// neutral rather than a penalty.
    pub fn calculate_match_score(
        &self,
        ledger: &LedgerTransaction,
        external: &ExternalTransaction,
    ) -> (f64, Vec<MatchDifference>) {
        let mut earned = 0.0;
        let mut weight_sum = 0.0;
        let mut differences = Vec::new();

        // Amount
        weight_sum += AMOUNT_WEIGHT;
        if ledger.amount == external.amount {
            earned += AMOUNT_WEIGHT;
        } else {
            let difference = (&ledger.amount - &external.amount).abs();
            let scale = ledger.amount.abs().max(external.amount.abs());
            if scale > *crate::ZERO && self.config.amount_tolerance_percentage > 0.0 {
                let relative = (&difference / &scale).to_f64().unwrap_or(1.0);
                if relative < self.config.amount_tolerance_percentage {
                    earned +=
                        AMOUNT_WEIGHT * (1.0 - relative / self.config.amount_tolerance_percentage);
                }
            }
            differences.push(MatchDifference::AmountDifference {
                ledger_amount: ledger.amount.clone(),
                external_amount: external.amount.clone(),
                difference,
            });
        }

        // Date
        weight_sum += DATE_WEIGHT;
        let days_diff = (ledger.date - external.date).num_days().abs();
        if days_diff == 0 {
            earned += DATE_WEIGHT;
        } else {
            if self.config.date_tolerance_days > 0 {
                let decay = 1.0 - (days_diff as f64 / self.config.date_tolerance_days as f64);
                earned += DATE_WEIGHT * decay.max(0.0);
            }
            differences.push(MatchDifference::DateDifference {
                ledger_date: ledger.date,
                external_date: external.date,
                days_diff,
            });
        }

        // Description
        weight_sum += DESCRIPTION_WEIGHT;
        let description_similarity =
            similarity::similarity(&ledger.description, &external.description);
        earned += DESCRIPTION_WEIGHT * description_similarity;
        if description_similarity < self.config.description_similarity_threshold {
            differences.push(MatchDifference::DescriptionDifference {
                ledger_description: ledger.description.clone(),
                external_description: external.description.clone(),
                similarity: description_similarity,
            });
        }

        // Direction
        weight_sum += ENTRY_TYPE_WEIGHT;
        if ledger.entry_type == external.entry_type {
            earned += ENTRY_TYPE_WEIGHT;
        } else {
            differences.push(MatchDifference::EntryTypeMismatch {
                ledger_entry_type: ledger.entry_type.clone(),
                external_entry_type: external.entry_type.clone(),
            });
        }

        // Reference - only weighed when both sides have one to compare
        if let (Some(ledger_reference), Some(external_reference)) = (
            normalized_reference(ledger.reference.as_deref()),
            normalized_reference(external.reference.as_deref()),
        ) {
            weight_sum += REFERENCE_WEIGHT;
            if ledger_reference == external_reference {
                earned += REFERENCE_WEIGHT;
            } else {
                differences.push(MatchDifference::ReferenceMismatch {
                    ledger_reference: ledger.reference.clone(),
                    external_reference: external.reference.clone(),
                });
            }
        }

        let score = if weight_sum > 0.0 {
            (earned / weight_sum).clamp(0.0, 1.0)
        } else {
            0.0
        };

        (score, differences)
    }

    /// Tally the items and wrap everything up into a report
    fn build_report(
        &self,
        ledger: Vec<LedgerTransaction>,
        external: Vec<ExternalTransaction>,
        external_source: ExternalSource,
        items: Vec<ReconciliationStatus>,
    ) -> ReconciliationReport {
        let mut matched_count = 0;
        let mut unmatched_ledger_count = 0;
        let mut unmatched_external_count = 0;
        let mut partial_match_count = 0;

        for item in &items {
            match item {
                ReconciliationStatus::Matched { .. } => matched_count += 1,
                ReconciliationStatus::UnmatchedLedger { .. } => unmatched_ledger_count += 1,
                ReconciliationStatus::UnmatchedExternal { .. } => unmatched_external_count += 1,
                ReconciliationStatus::PartialMatch { .. } => partial_match_count += 1,
            }
        }

        let summary = summarize(&ledger, &external, matched_count);

        let mut account_ids: Vec<String> = ledger
            .iter()
            .map(|transaction| transaction.account_id.clone())
            .collect();
        account_ids.sort();
        account_ids.dedup();

        ReconciliationReport {
            id: uuid::Uuid::new_v4(),
            created_at: chrono::Utc::now().naive_utc(),
            period_start: period_bound(&ledger, &external, true),
            period_end: period_bound(&ledger, &external, false),
            external_source,
            account_ids,
            total_ledger_transactions: ledger.len(),
            total_external_transactions: external.len(),
            matched_count,
            unmatched_ledger_count,
            unmatched_external_count,
            partial_match_count,
            reconciliation_items: items,
            summary,
        }
    }
}

/// Whether a pair must be seen by a person regardless of how well it scores.
///
/// A gap in the amount means money is unaccounted for, and a flipped direction means the entry
/// went the wrong way. Both are reconciliation findings in their own right, so neither is ever
/// swept into an automatic match no matter how well the rest of the record agrees.
fn requires_review(differences: &[MatchDifference]) -> bool {
    differences.iter().any(|difference| {
        matches!(
            difference,
            MatchDifference::AmountDifference { .. } | MatchDifference::EntryTypeMismatch { .. }
        )
    })
}

/// Key identifying transactions that agree exactly, with the amount rendered in canonical form so
/// that `1.0` and `1.00` land in the same bucket
type ExactKey = (NaiveDate, String, EntryType);

/// Build an [`ExactKey`]
fn exact_key(date: NaiveDate, amount: &BigDecimal, entry_type: &EntryType) -> ExactKey {
    (date, amount.normalized().to_string(), entry_type.clone())
}

/// Trim and upper-case a reference, treating blank references as absent
fn normalized_reference(reference: Option<&str>) -> Option<String> {
    let trimmed = reference?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_uppercase())
    }
}

/// Net movement across a set of transactions, counting credits positive
fn net_balance<'a>(
    transactions: impl Iterator<Item = (&'a EntryType, &'a BigDecimal)>,
) -> BigDecimal {
    transactions.fold(
        BigDecimal::from(0),
        |total, (entry_type, amount)| match entry_type {
            EntryType::Credit => total + amount,
            EntryType::Debit => total - amount,
        },
    )
}

/// Aggregate balances, match rate and overall confidence
fn summarize(
    ledger: &[LedgerTransaction],
    external: &[ExternalTransaction],
    matched_count: usize,
) -> ReconciliationSummary {
    let ledger_balance = net_balance(
        ledger
            .iter()
            .map(|transaction| (&transaction.entry_type, &transaction.amount)),
    );
    let external_balance = net_balance(
        external
            .iter()
            .map(|transaction| (&transaction.entry_type, &transaction.amount)),
    );
    let difference = &ledger_balance - &external_balance;

    let total = ledger.len().max(external.len());
    let match_rate = if total > 0 {
        matched_count as f64 / total as f64
    } else {
        0.0
    };

    let scale = ledger_balance.abs().max(external_balance.abs());
    let balance_accuracy = if scale == *crate::ZERO {
        1.0
    } else {
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

/// Earliest (`start`) or latest date across both sides, falling back to today when both are empty
fn period_bound(
    ledger: &[LedgerTransaction],
    external: &[ExternalTransaction],
    start: bool,
) -> NaiveDate {
    let dates = ledger
        .iter()
        .map(|transaction| transaction.date)
        .chain(external.iter().map(|transaction| transaction.date));

    let bound = if start { dates.min() } else { dates.max() };
    bound.unwrap_or_else(|| chrono::Utc::now().date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn source() -> ExternalSource {
        ExternalSource::BankStatement {
            bank_name: "Test Bank".to_string(),
            account_number: "123456".to_string(),
        }
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 11, day).unwrap()
    }

    fn amount(value: &str) -> BigDecimal {
        BigDecimal::from_str(value).unwrap()
    }

    fn ledger_txn(id: &str, day: u32, value: &str, description: &str) -> LedgerTransaction {
        LedgerTransaction::new(
            id.to_string(),
            date(day),
            amount(value),
            description.to_string(),
            EntryType::Debit,
            "bank".to_string(),
        )
    }

    fn external_txn(id: &str, day: u32, value: &str, description: &str) -> ExternalTransaction {
        ExternalTransaction::new(
            id.to_string(),
            date(day),
            amount(value),
            description.to_string(),
            EntryType::Debit,
            source(),
        )
    }

    fn matched_pairs(report: &ReconciliationReport) -> Vec<(String, String)> {
        report
            .reconciliation_items
            .iter()
            .filter_map(|item| match item {
                ReconciliationStatus::Matched {
                    ledger_id,
                    external_id,
                    ..
                } => Some((ledger_id.clone(), external_id.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_exact_match() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment from customer")],
            vec![external_txn("ext-1", 15, "1000", "Payment from customer")],
            source(),
        );

        assert_eq!(report.matched_count, 1);
        assert_eq!(report.unmatched_ledger_count, 0);
        assert_eq!(report.unmatched_external_count, 0);
        assert_eq!(report.partial_match_count, 0);
        assert_eq!(report.summary.match_rate, 1.0);
        assert_eq!(report.summary.difference, amount("0"));
        assert_eq!(report.account_ids, vec!["bank".to_string()]);
        assert!(report.is_fully_reconciled());
        assert_eq!(report.period_start, date(15));
        assert_eq!(report.period_end, date(15));
    }

    #[test]
    fn test_scale_differences_do_not_break_exact_match() {
        // 1000 and 1000.00 are the same amount and must land in the same bucket
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Rent")],
            vec![external_txn("ext-1", 15, "1000.00", "Rent")],
            source(),
        );

        assert_eq!(report.matched_count, 1);
    }

    #[test]
    fn test_reference_match_beats_a_same_day_decoy() {
        let ledger = vec![ledger_txn("txn-1", 15, "1000", "Payment").with_reference("utr-1")];
        let external = vec![
            // Same day and amount, so the exact pass would happily take it
            external_txn("ext-decoy", 15, "1000", "Payment").with_reference("UTR-9"),
            // Settled a day late with a wholly different narration, but the UTR agrees
            external_txn("ext-real", 16, "1000", "NEFT INWARD 0099").with_reference("UTR-1"),
        ];

        let report = ReconciliationEngine::default().reconcile(ledger, external, source());

        assert_eq!(
            matched_pairs(&report),
            vec![("txn-1".to_string(), "ext-real".to_string())]
        );
        assert_eq!(report.unmatched_external_count, 1);
    }

    #[test]
    fn test_ambiguous_reference_falls_through_to_scoring() {
        let ledger = vec![ledger_txn("txn-1", 15, "1000", "Payment").with_reference("UTR-1")];
        let external = vec![
            external_txn("ext-a", 15, "1000", "Payment").with_reference("UTR-1"),
            external_txn("ext-b", 20, "1000", "Payment").with_reference("UTR-1"),
        ];

        let report = ReconciliationEngine::default().reconcile(ledger, external, source());

        // Two references collide, so the reference pass declines and the exact pass picks the
        // one that also agrees on the date.
        assert_eq!(
            matched_pairs(&report),
            vec![("txn-1".to_string(), "ext-a".to_string())]
        );
    }

    #[test]
    fn test_date_within_tolerance_is_a_partial_match() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment from customer")],
            vec![external_txn("ext-1", 16, "1000", "Payment from customer")],
            source(),
        );

        assert_eq!(report.partial_match_count, 1);
        assert_eq!(report.matched_count, 0);

        let ReconciliationStatus::PartialMatch {
            differences,
            match_score,
            ..
        } = &report.reconciliation_items[0]
        else {
            panic!(
                "expected a partial match, got {:?}",
                report.reconciliation_items[0]
            );
        };
        assert!(*match_score > 0.8, "score was {match_score}");
        assert!(differences.iter().any(|difference| matches!(
            difference,
            MatchDifference::DateDifference { days_diff: 1, .. }
        )));
    }

    #[test]
    fn test_date_beyond_tolerance_is_unmatched_but_suggested() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment from customer")],
            vec![external_txn("ext-1", 25, "1000", "Payment from customer")],
            source(),
        );

        assert_eq!(report.matched_count, 0);
        assert_eq!(report.partial_match_count, 0);
        assert_eq!(report.unmatched_ledger_count, 1);
        assert_eq!(report.unmatched_external_count, 1);

        let ReconciliationStatus::UnmatchedLedger {
            possible_matches, ..
        } = &report.reconciliation_items[0]
        else {
            panic!("expected an unmatched ledger transaction");
        };
        assert_eq!(possible_matches.len(), 1);
        assert_eq!(possible_matches[0].counterpart_id, "ext-1");
    }

    #[test]
    fn test_outside_candidate_window_is_not_even_suggested() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 1, "1000", "Payment from customer")],
            vec![ExternalTransaction::new(
                "ext-1".to_string(),
                NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
                amount("1000"),
                "Payment from customer".to_string(),
                EntryType::Debit,
                source(),
            )],
            source(),
        );

        let ReconciliationStatus::UnmatchedLedger {
            possible_matches, ..
        } = &report.reconciliation_items[0]
        else {
            panic!("expected an unmatched ledger transaction");
        };
        assert!(possible_matches.is_empty());
    }

    #[test]
    fn test_amount_difference_always_needs_review() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment from customer")],
            vec![external_txn(
                "ext-1",
                15,
                "1000.50",
                "Payment from customer",
            )],
            source(),
        );

        assert_eq!(report.partial_match_count, 1);

        let ReconciliationStatus::PartialMatch {
            differences,
            match_score,
            auto_resolvable,
            ..
        } = &report.reconciliation_items[0]
        else {
            panic!("expected a partial match");
        };

        // Scores high enough to auto-match on the raw numbers, but money is missing
        assert!(*match_score > 0.95, "score was {match_score}");
        assert!(!auto_resolvable);
        assert!(differences.iter().any(|difference| matches!(
            difference,
            MatchDifference::AmountDifference { difference, .. } if *difference == amount("0.50")
        )));
        assert_eq!(report.summary.difference, amount("0.50"));
    }

    #[test]
    fn test_opposite_direction_always_needs_review() {
        let mut external = external_txn("ext-1", 15, "1000", "Payment from customer");
        external.entry_type = EntryType::Credit;

        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment from customer")],
            vec![external],
            source(),
        );

        assert_eq!(report.partial_match_count, 1);
        let ReconciliationStatus::PartialMatch {
            differences,
            auto_resolvable,
            ..
        } = &report.reconciliation_items[0]
        else {
            panic!("expected a partial match");
        };
        assert!(!auto_resolvable);
        assert!(differences
            .iter()
            .any(|difference| matches!(difference, MatchDifference::EntryTypeMismatch { .. })));
    }

    #[test]
    fn test_zero_amounts_do_not_panic() {
        let report = ReconciliationEngine::default().reconcile(
            vec![
                ledger_txn("txn-0", 15, "0", "Nil adjustment"),
                ledger_txn("txn-1", 15, "0", "Opening float"),
            ],
            vec![
                external_txn("ext-0", 15, "0", "Nil adjustment"),
                external_txn("ext-1", 15, "500", "Unrelated"),
            ],
            source(),
        );

        // Zero matches zero exactly; zero against 500 is simply not a match.
        assert_eq!(report.matched_count, 1);
        assert_eq!(report.unmatched_ledger_count, 1);
        assert_eq!(report.unmatched_external_count, 1);
        assert_eq!(report.summary.ledger_balance, amount("0"));
    }

    #[test]
    fn test_result_is_independent_of_input_order() {
        let ledger = vec![
            ledger_txn("txn-1", 15, "1000", "Acme Ltd invoice"),
            ledger_txn("txn-2", 16, "250", "Office rent"),
            ledger_txn("txn-3", 17, "75", "Courier charges"),
            ledger_txn("txn-4", 18, "9999", "Only in the ledger"),
        ];
        let external = vec![
            external_txn("ext-1", 15, "1000", "NEFT/ACME LTD/0012"),
            external_txn("ext-2", 16, "250", "Office rent"),
            external_txn("ext-3", 18, "75", "Courier charges"),
            external_txn("ext-4", 19, "4242", "Only on the statement"),
        ];

        let engine = ReconciliationEngine::default();
        let forward = engine.reconcile(ledger.clone(), external.clone(), source());

        let reversed_ledger: Vec<_> = ledger.into_iter().rev().collect();
        let reversed_external: Vec<_> = external.into_iter().rev().collect();
        let backward = engine.reconcile(reversed_ledger, reversed_external, source());

        assert_eq!(
            forward.reconciliation_items, backward.reconciliation_items,
            "reconciliation must not depend on input ordering"
        );
        assert_eq!(forward.summary, backward.summary);
    }

    #[test]
    fn test_duplicate_externals_are_matched_only_once() {
        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "Payment")],
            vec![
                external_txn("ext-a", 15, "1000", "Payment"),
                external_txn("ext-b", 15, "1000", "Payment"),
            ],
            source(),
        );

        assert_eq!(
            matched_pairs(&report),
            vec![("txn-1".to_string(), "ext-a".to_string())]
        );
        assert_eq!(report.unmatched_external_count, 1);

        // The counterpart is spoken for, so it must not be offered as a suggestion either.
        let ReconciliationStatus::UnmatchedExternal {
            possible_matches, ..
        } = &report.reconciliation_items[1]
        else {
            panic!("expected an unmatched external transaction");
        };
        assert!(possible_matches.is_empty());
    }

    #[test]
    fn test_empty_inputs() {
        let report = ReconciliationEngine::default().reconcile(Vec::new(), Vec::new(), source());

        assert_eq!(report.total_ledger_transactions, 0);
        assert_eq!(report.total_external_transactions, 0);
        assert!(report.reconciliation_items.is_empty());
        assert_eq!(report.summary.match_rate, 0.0);
        assert_eq!(report.summary.confidence_score, 0.5);
        assert_eq!(report.period_start, report.period_end);
        assert!(report.account_ids.is_empty());
    }

    #[test]
    fn test_summary_balances_are_credit_positive() {
        let mut ledger_credit = ledger_txn("txn-1", 15, "1000", "Sale proceeds");
        ledger_credit.entry_type = EntryType::Credit;
        let ledger_debit = ledger_txn("txn-2", 16, "400", "Bank charges");

        let mut external_credit = external_txn("ext-1", 15, "1000", "Sale proceeds");
        external_credit.entry_type = EntryType::Credit;

        let report = ReconciliationEngine::default().reconcile(
            vec![ledger_credit, ledger_debit],
            vec![external_credit],
            source(),
        );

        assert_eq!(report.summary.ledger_balance, amount("600"));
        assert_eq!(report.summary.external_balance, amount("1000"));
        assert_eq!(report.summary.difference, amount("-400"));
        assert_eq!(report.summary.match_rate, 0.5);
    }

    #[test]
    fn test_suggestions_are_ranked_and_capped() {
        let config = ReconciliationConfig {
            max_suggestions: 2,
            ..ReconciliationConfig::default()
        };

        // Every candidate is off on the amount, so none can pair; they differ only in narration.
        let external = vec![
            external_txn("ext-1", 15, "900", "acme ltd payment"),
            external_txn("ext-2", 15, "900", "acme ltd"),
            external_txn("ext-3", 15, "900", "acme corp payment"),
            external_txn("ext-4", 15, "900", "globex payment"),
            external_txn("ext-5", 15, "900", "zzz qqq"),
        ];

        let report = ReconciliationEngine::new(config).reconcile(
            vec![ledger_txn("txn-1", 15, "1000", "acme ltd payment")],
            external,
            source(),
        );

        let ReconciliationStatus::UnmatchedLedger {
            possible_matches, ..
        } = &report.reconciliation_items[0]
        else {
            panic!("expected an unmatched ledger transaction");
        };

        assert_eq!(possible_matches.len(), 2);
        assert_eq!(possible_matches[0].counterpart_id, "ext-1");
        assert!(possible_matches[0].match_score > possible_matches[1].match_score);
    }

    #[test]
    fn test_global_assignment_prefers_the_stronger_pairing() {
        // Both ledger rows could take ext-1 on a date-shifted score, but only txn-2 agrees with it
        // exactly. A first-fit loop over the ledger would hand ext-1 to txn-1 and strand txn-2.
        let ledger = vec![
            ledger_txn("txn-1", 14, "1000", "Payment from customer"),
            ledger_txn("txn-2", 15, "1000", "Payment from customer"),
        ];
        let external = vec![external_txn("ext-1", 15, "1000", "Payment from customer")];

        let report = ReconciliationEngine::default().reconcile(ledger, external, source());

        assert_eq!(
            matched_pairs(&report),
            vec![("txn-2".to_string(), "ext-1".to_string())]
        );
        assert_eq!(report.unmatched_ledger_count, 1);
    }

    #[test]
    fn test_missing_reference_is_neutral_not_a_penalty() {
        let engine = ReconciliationEngine::default();
        let ledger = ledger_txn("txn-1", 16, "1000", "Payment from customer");
        let external = external_txn("ext-1", 15, "1000", "Payment from customer");

        let (without_reference, _) = engine.calculate_match_score(&ledger, &external);
        let (with_matching_reference, _) = engine.calculate_match_score(
            &ledger.clone().with_reference("UTR-1"),
            &external.clone().with_reference("utr-1"),
        );
        let (with_conflicting_reference, differences) = engine.calculate_match_score(
            &ledger.with_reference("UTR-1"),
            &external.with_reference("UTR-2"),
        );

        assert!(with_matching_reference > without_reference);
        assert!(with_conflicting_reference < without_reference);
        assert!(differences
            .iter()
            .any(|difference| matches!(difference, MatchDifference::ReferenceMismatch { .. })));
    }
}
