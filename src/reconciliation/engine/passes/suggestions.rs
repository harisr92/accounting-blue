//! Pass 5: report what is left, each with its best still-available counterparts

use super::super::state::{Candidate, MatchState, Sides};
use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::types::{PartialMatch, ReconciliationStatus};

/// Suggestion lists, one per row on each side
struct Suggestions {
    ledger: Vec<Vec<PartialMatch>>,
    external: Vec<Vec<PartialMatch>>,
}

/// Append a suggestion unless the list is already full
fn push_capped(
    list: &mut Vec<PartialMatch>,
    max: usize,
    counterpart_id: &str,
    candidate: &Candidate,
) {
    if list.len() < max {
        list.push(PartialMatch {
            counterpart_id: counterpart_id.to_string(),
            match_score: candidate.score,
            differences: candidate.differences.clone(),
        });
    }
}

/// Collect up to `max_suggestions` counterparts per row from the free candidate pairs
fn suggest(
    config: &ReconciliationConfig,
    sides: &Sides,
    candidates: &[Candidate],
    state: &MatchState,
) -> Suggestions {
    let max = config.max_suggestions;
    let empty = Suggestions {
        ledger: vec![Vec::new(); sides.ledger.len()],
        external: vec![Vec::new(); sides.external.len()],
    };

    // `candidates` is already sorted by score descending, so each list comes out sorted too.
    // Never suggest a counterpart that is already spoken for.
    candidates
        .iter()
        .filter(|c| state.is_free(c.ledger_index, c.external_index))
        .fold(empty, |mut suggestions, c| {
            let external_id = &sides.external[c.external_index].id;
            let ledger_id = &sides.ledger[c.ledger_index].id;
            push_capped(&mut suggestions.ledger[c.ledger_index], max, external_id, c);
            push_capped(
                &mut suggestions.external[c.external_index],
                max,
                ledger_id,
                c,
            );
            suggestions
        })
}

/// Report every unclaimed row as unmatched, carrying its suggestions
pub(in crate::reconciliation::engine) fn collect_unmatched(
    config: &ReconciliationConfig,
    sides: &Sides,
    candidates: &[Candidate],
    state: &mut MatchState,
) {
    let Suggestions {
        mut ledger,
        mut external,
    } = suggest(config, sides, candidates, state);

    let unmatched_ledger: Vec<ReconciliationStatus> = sides
        .free_ledger(state)
        .map(|index| ReconciliationStatus::UnmatchedLedger {
            ledger_id: sides.ledger[index].id.clone(),
            possible_matches: std::mem::take(&mut ledger[index]),
        })
        .collect();
    let unmatched_external: Vec<ReconciliationStatus> = sides
        .free_external(state)
        .map(|index| ReconciliationStatus::UnmatchedExternal {
            external_id: sides.external[index].id.clone(),
            possible_matches: std::mem::take(&mut external[index]),
        })
        .collect();

    state.items.extend(unmatched_ledger);
    state.items.extend(unmatched_external);
}
