//! Bookkeeping shared by the matching passes

use crate::reconciliation::scoring::EXACT_MATCH_SCORE;
use crate::reconciliation::types::{
    ExternalTransaction, LedgerTransaction, MatchDifference, ReconciliationStatus,
};

/// Indices of `items` sorted by the id `key` returns
fn sorted_by_id<T>(items: &[T], key: impl Fn(&T) -> &str) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| key(&items[a]).cmp(key(&items[b])));
    order
}

/// Both sides of a reconciliation, plus the id-sorted traversal orders.
///
/// Every pass walks these orders rather than the slices directly, so a given pair of inputs
/// always produces the same report however they happened to be sorted on the way in.
pub(super) struct Sides<'a> {
    pub ledger: &'a [LedgerTransaction],
    pub external: &'a [ExternalTransaction],
    pub ledger_order: Vec<usize>,
    pub external_order: Vec<usize>,
}

impl<'a> Sides<'a> {
    pub fn new(ledger: &'a [LedgerTransaction], external: &'a [ExternalTransaction]) -> Self {
        Self {
            ledger,
            external,
            ledger_order: sorted_by_id(ledger, |t| &t.id),
            external_order: sorted_by_id(external, |t| &t.id),
        }
    }

    /// Unclaimed ledger indices, in id order
    pub fn free_ledger<'s>(&'s self, state: &'s MatchState) -> impl Iterator<Item = usize> + 's {
        self.ledger_order
            .iter()
            .copied()
            .filter(|&index| state.is_ledger_free(index))
    }

    /// Unclaimed external indices, in id order
    pub fn free_external<'s>(&'s self, state: &'s MatchState) -> impl Iterator<Item = usize> + 's {
        self.external_order
            .iter()
            .copied()
            .filter(|&index| state.is_external_free(index))
    }
}

/// What has been claimed so far, and the report items produced along the way
pub(super) struct MatchState {
    ledger_taken: Vec<bool>,
    external_taken: Vec<bool>,
    pub items: Vec<ReconciliationStatus>,
}

impl MatchState {
    pub fn new(sides: &Sides) -> Self {
        Self {
            ledger_taken: vec![false; sides.ledger.len()],
            external_taken: vec![false; sides.external.len()],
            items: Vec::new(),
        }
    }

    pub fn is_ledger_free(&self, index: usize) -> bool {
        !self.ledger_taken[index]
    }

    pub fn is_external_free(&self, index: usize) -> bool {
        !self.external_taken[index]
    }

    pub fn is_free(&self, ledger_index: usize, external_index: usize) -> bool {
        self.is_ledger_free(ledger_index) && self.is_external_free(external_index)
    }

    pub fn claim(&mut self, ledger_index: usize, external_index: usize) {
        self.ledger_taken[ledger_index] = true;
        self.external_taken[external_index] = true;
    }

    /// Claim a pair that agrees conclusively and report it as matched
    pub fn record_exact_match(
        &mut self,
        sides: &Sides,
        ledger_index: usize,
        external_index: usize,
    ) {
        self.claim(ledger_index, external_index);
        self.items.push(ReconciliationStatus::Matched {
            ledger_id: sides.ledger[ledger_index].id.clone(),
            external_id: sides.external[external_index].id.clone(),
            match_score: EXACT_MATCH_SCORE,
        });
    }
}

/// A scored pairing considered during the fuzzy passes
pub(super) struct Candidate {
    pub ledger_index: usize,
    pub external_index: usize,
    pub score: f64,
    pub differences: Vec<MatchDifference>,
}
