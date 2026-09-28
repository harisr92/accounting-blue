//! Pair scoring: how alike a ledger and an external transaction are, and how they differ
//!
//! Each dimension (amount, date, description, direction, reference) is scored on its own; the
//! pair's score is the weight earned over the weight available.

use bigdecimal::{Signed, ToPrimitive};

use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::similarity;
use crate::reconciliation::types::{ExternalTransaction, LedgerTransaction, MatchDifference};

/// Score given to a pair matched by reference or exact agreement
pub const EXACT_MATCH_SCORE: f64 = 1.0;

/// One dimension's contribution to a pair's score
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DimensionScore {
    /// Weight this dimension carries
    pub(super) weight: f64,
    /// Share of the weight earned, in `0.0..=weight`
    pub(super) earned: f64,
    /// How the two sides disagree on this dimension, if they do
    pub(super) difference: Option<MatchDifference>,
}

impl DimensionScore {
    fn full(weight: f64) -> Self {
        Self {
            weight,
            earned: weight,
            difference: None,
        }
    }
}

/// Score how alike two transactions are, in `0.0..=1.0`, along with every way they disagree
#[must_use]
pub fn score_pair(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> (f64, Vec<MatchDifference>) {
    let dimensions = [
        Some(score_amount(config, ledger, external)),
        Some(score_date(config, ledger, external)),
        Some(score_description(config, ledger, external)),
        Some(score_entry_type(config, ledger, external)),
        score_reference(config, ledger, external),
    ];

    let (earned, weight_sum, differences) = dimensions.into_iter().flatten().fold(
        (0.0, 0.0, Vec::new()),
        |(earned, weight_sum, mut differences), dimension| {
            differences.extend(dimension.difference);
            (
                earned + dimension.earned,
                weight_sum + dimension.weight,
                differences,
            )
        },
    );

    let score = if weight_sum > 0.0 {
        (earned / weight_sum).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (score, differences)
}

/// Full credit for equal amounts, decaying linearly to nothing at the tolerance
pub(super) fn score_amount(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> DimensionScore {
    let weight = config.weights.amount;
    if ledger.amount == external.amount {
        return DimensionScore::full(weight);
    }

    let difference = (&ledger.amount - &external.amount).abs();
    let scale = ledger.amount.abs().max(external.amount.abs());
    let tolerance = config.amount_tolerance_percentage;

    // A ratio too large for f64 counts as a 100% gap
    let relative = (scale.is_positive() && tolerance > 0.0)
        .then(|| (&difference / &scale).to_f64().unwrap_or(1.0))
        .filter(|&relative| relative < tolerance);

    DimensionScore {
        weight,
        earned: relative.map_or(0.0, |relative| weight * (1.0 - relative / tolerance)),
        difference: Some(MatchDifference::AmountDifference {
            ledger_amount: ledger.amount.clone(),
            external_amount: external.amount.clone(),
            difference,
        }),
    }
}

/// Full credit on the same day, decaying linearly to nothing at the date tolerance
#[allow(clippy::cast_precision_loss)] // day counts are tiny
fn score_date(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> DimensionScore {
    let weight = config.weights.date;
    let days_diff = (ledger.date - external.date).num_days().abs();
    if days_diff == 0 {
        return DimensionScore::full(weight);
    }

    let tolerance = config.date_tolerance_days;
    let earned = if tolerance > 0 {
        let decay = 1.0 - (days_diff as f64 / tolerance as f64);
        weight * decay.max(0.0)
    } else {
        0.0
    };

    DimensionScore {
        weight,
        earned,
        difference: Some(MatchDifference::DateDifference {
            ledger_date: ledger.date,
            external_date: external.date,
            days_diff,
        }),
    }
}

/// Credit in proportion to description similarity
fn score_description(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> DimensionScore {
    let weight = config.weights.description;
    let similarity = similarity::similarity(&ledger.description, &external.description);

    DimensionScore {
        weight,
        earned: weight * similarity,
        difference: (similarity < config.description_similarity_threshold).then(|| {
            MatchDifference::DescriptionDifference {
                ledger_description: ledger.description.clone(),
                external_description: external.description.clone(),
                similarity,
            }
        }),
    }
}

/// All or nothing on direction
fn score_entry_type(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> DimensionScore {
    let weight = config.weights.entry_type;
    if ledger.entry_type == external.entry_type {
        return DimensionScore::full(weight);
    }

    DimensionScore {
        weight,
        earned: 0.0,
        difference: Some(MatchDifference::EntryTypeMismatch {
            ledger_entry_type: ledger.entry_type,
            external_entry_type: external.entry_type,
        }),
    }
}

/// All or nothing on reference, and only weighed when both sides have one
fn score_reference(
    config: &ReconciliationConfig,
    ledger: &LedgerTransaction,
    external: &ExternalTransaction,
) -> Option<DimensionScore> {
    let ledger_reference = normalized_reference(ledger.reference.as_deref())?;
    let external_reference = normalized_reference(external.reference.as_deref())?;
    let weight = config.weights.reference;

    if ledger_reference == external_reference {
        return Some(DimensionScore::full(weight));
    }

    Some(DimensionScore {
        weight,
        earned: 0.0,
        difference: Some(MatchDifference::ReferenceMismatch {
            ledger_reference: ledger.reference.clone(),
            external_reference: external.reference.clone(),
        }),
    })
}

/// Trim and upper-case a reference, treating blank references as absent
pub(crate) fn normalized_reference(reference: Option<&str>) -> Option<String> {
    let trimmed = reference?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_uppercase())
}

/// Whether a pair must be seen by a person regardless of how well it scores.
///
/// A gap in the amount means money is unaccounted for, and a flipped direction means the entry
/// went the wrong way. Both are reconciliation findings in their own right, so neither is ever
/// swept into an automatic match no matter how well the rest of the record agrees.
pub(crate) fn requires_review(differences: &[MatchDifference]) -> bool {
    differences.iter().any(|difference| {
        matches!(
            difference,
            MatchDifference::AmountDifference { .. } | MatchDifference::EntryTypeMismatch { .. }
        )
    })
}
