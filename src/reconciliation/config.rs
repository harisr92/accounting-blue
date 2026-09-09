//! Tuning knobs for the reconciliation engine

/// Thresholds and tolerances that control matching
///
/// The defaults suit Indian bank statements, where settlement lag of a day or two is normal and
/// narrations are noisy. Note that with the defaults a date-shifted pair that carries no reference
/// number lands as a [`PartialMatch`](crate::reconciliation::ReconciliationStatus::PartialMatch)
/// rather than an outright match — widen `date_tolerance_days` or lower `auto_match_threshold` if
/// you would rather accept those automatically. Pairs that agree on reference number, amount and
/// direction always match outright, whatever their dates.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationConfig {
    /// How many days apart two records may be dated and still score on the date dimension
    pub date_tolerance_days: i64,
    /// Largest relative amount gap that still earns credit, as a fraction (`0.01` is 1%)
    pub amount_tolerance_percentage: f64,
    /// Description similarity below which a
    /// [`DescriptionDifference`](crate::reconciliation::MatchDifference::DescriptionDifference)
    /// is recorded
    pub description_similarity_threshold: f64,
    /// Score at or above which a pair is matched without review
    pub auto_match_threshold: f64,
    /// Score at or above which a pair is paired up at all
    pub partial_match_threshold: f64,
    /// Score at or above which a partial match is flagged `auto_resolvable`
    pub auto_resolve_threshold: f64,
    /// Score at or above which a pair is offered as a suggestion for an unmatched transaction
    pub suggestion_threshold: f64,
    /// How many suggestions to keep per unmatched transaction
    pub max_suggestions: usize,
    /// Pairs dated further apart than this are never scored, bounding the comparison sweep
    pub candidate_window_days: i64,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            date_tolerance_days: 2,
            amount_tolerance_percentage: 0.01,
            description_similarity_threshold: 0.8,
            auto_match_threshold: 0.95,
            partial_match_threshold: 0.70,
            auto_resolve_threshold: 0.90,
            suggestion_threshold: 0.30,
            max_suggestions: 5,
            candidate_window_days: 30,
        }
    }
}
