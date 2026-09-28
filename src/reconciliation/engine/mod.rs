//! The matching engine
//!
//! [`ReconciliationEngine::reconcile`] is pure and synchronous: it takes both sides in memory and
//! returns a [`ReconciliationReport`]. Loading transactions and persisting the report are the
//! caller's job, via [`ReconciliationStorage`](crate::reconciliation::ReconciliationStorage).
//!
//! The work is split by responsibility: a private `state` module tracks what has been claimed,
//! each module under `passes` is one matching pass, [`scoring`](crate::reconciliation::scoring)
//! scores a pair, and a private `report` module tallies the result.

mod passes;
mod state;

use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::report::build_report;
use crate::reconciliation::scoring::score_pair;
use crate::reconciliation::types::{
    ExternalSource, ExternalTransaction, LedgerTransaction, MatchDifference, ReconciliationReport,
    ReconciliationStatus,
};
use state::{MatchState, Sides};

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
/// let report = ReconciliationEngine::default().reconcile(&ledger, &external, source);
/// assert_eq!(report.matched_count, 1);
/// assert!(report.is_fully_reconciled());
/// ```
#[derive(Debug, Clone, Default)]
pub struct ReconciliationEngine {
    config: ReconciliationConfig,
}

impl ReconciliationEngine {
    /// Create an engine with the given configuration
    #[must_use]
    pub fn new(config: ReconciliationConfig) -> Self {
        Self { config }
    }

    /// The configuration in use
    #[must_use]
    pub fn config(&self) -> &ReconciliationConfig {
        &self.config
    }

    /// Match both sides and produce a report.
    ///
    /// Matching runs in five passes: reference numbers, exact date/amount/direction, a scored
    /// sweep of what remains, best-first assignment of the scored pairs, and finally suggestions
    /// for whatever is still unmatched. The result does not depend on the order of either input.
    #[must_use]
    pub fn reconcile(
        &self,
        ledger: &[LedgerTransaction],
        external: &[ExternalTransaction],
        external_source: ExternalSource,
    ) -> ReconciliationReport {
        build_report(
            ledger,
            external,
            external_source,
            self.match_items(ledger, external),
        )
    }

    /// Run the passes in order and return the report items
    fn match_items(
        &self,
        ledger: &[LedgerTransaction],
        external: &[ExternalTransaction],
    ) -> Vec<ReconciliationStatus> {
        let sides = Sides::new(ledger, external);
        let mut state = MatchState::new(&sides);

        passes::match_by_reference(&sides, &mut state);
        passes::match_exactly(&sides, &mut state);

        let candidates = passes::score_remaining(&self.config, &sides, &state);
        passes::assign_best_first(&self.config, &sides, &candidates, &mut state);
        passes::collect_unmatched(&self.config, &sides, &candidates, &mut state);

        state.items
    }

    /// Score how alike two transactions are, in `0.0..=1.0`, along with every way they disagree.
    ///
    /// Each dimension contributes its weight (see
    /// [`ScoringWeights`](crate::reconciliation::ScoringWeights)) scaled by how well it agrees.
    /// The reference dimension is dropped from the total when either side lacks a reference, so a
    /// missing reference is neutral rather than a penalty.
    #[must_use]
    pub fn calculate_match_score(
        &self,
        ledger: &LedgerTransaction,
        external: &ExternalTransaction,
    ) -> (f64, Vec<MatchDifference>) {
        score_pair(&self.config, ledger, external)
    }
}

#[cfg(test)]
mod tests;
