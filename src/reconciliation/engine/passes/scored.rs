//! Passes 3 and 4: score what is left, then assign the best pairs first

use std::cmp::Ordering;

use super::super::state::{Candidate, MatchState, Sides};
use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::scoring::{requires_review, score_pair};
use crate::reconciliation::types::ReconciliationStatus;

/// Score every still-plausible pair once, feeding both the assignment and the suggestion lists.
///
/// Pairs dated further apart than `candidate_window_days`, or scoring below
/// `suggestion_threshold`, are dropped. The result is sorted best-first, ties broken by ids.
pub(in crate::reconciliation::engine) fn score_remaining(
    config: &ReconciliationConfig,
    sides: &Sides,
    state: &MatchState,
) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = sides
        .free_ledger(state)
        .flat_map(|ledger_index| {
            sides
                .free_external(state)
                .map(move |external_index| (ledger_index, external_index))
        })
        .filter(|&(l, e)| {
            let days_apart = (sides.ledger[l].date - sides.external[e].date).num_days();
            days_apart.abs() <= config.candidate_window_days
        })
        .filter_map(|(ledger_index, external_index)| {
            let (score, differences) = score_pair(
                config,
                &sides.ledger[ledger_index],
                &sides.external[external_index],
            );
            (score >= config.suggestion_threshold).then_some(Candidate {
                ledger_index,
                external_index,
                score,
                differences,
            })
        })
        .collect();

    candidates.sort_by(|a, b| best_first(sides, a, b));
    candidates
}

/// Highest score first, then by ledger id, then by external id
fn best_first(sides: &Sides, a: &Candidate, b: &Candidate) -> Ordering {
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
}

/// Walk the scored pairs best-first, taking each whose both sides are still free.
///
/// Assigning globally rather than per ledger row stops a mediocre pairing from claiming a
/// counterpart that a stronger pairing later in the input wanted. A pair that disagrees on money
/// or direction is always reported as a partial match, however high it scores - see
/// [`requires_review`].
pub(in crate::reconciliation::engine) fn assign_best_first(
    config: &ReconciliationConfig,
    sides: &Sides,
    candidates: &[Candidate],
    state: &mut MatchState,
) {
    let qualifying = candidates
        .iter()
        .take_while(|candidate| candidate.score >= config.partial_match_threshold);

    for candidate in qualifying {
        if state.is_free(candidate.ledger_index, candidate.external_index) {
            state.claim(candidate.ledger_index, candidate.external_index);
            state.items.push(classify(config, sides, candidate));
        }
    }
}

/// A full match when the score clears `auto_match_threshold` and nothing needs review
fn classify(
    config: &ReconciliationConfig,
    sides: &Sides,
    candidate: &Candidate,
) -> ReconciliationStatus {
    let ledger_id = sides.ledger[candidate.ledger_index].id.clone();
    let external_id = sides.external[candidate.external_index].id.clone();
    let needs_review = requires_review(&candidate.differences);

    if candidate.score >= config.auto_match_threshold && !needs_review {
        ReconciliationStatus::Matched {
            ledger_id,
            external_id,
            match_score: candidate.score,
        }
    } else {
        ReconciliationStatus::PartialMatch {
            ledger_id,
            external_id,
            match_score: candidate.score,
            differences: candidate.differences.clone(),
            auto_resolvable: candidate.score >= config.auto_resolve_threshold && !needs_review,
        }
    }
}
