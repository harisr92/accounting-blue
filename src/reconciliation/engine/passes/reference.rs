//! Pass 1: a shared reference number, backed by an equal amount and direction, is conclusive

use std::collections::HashMap;

use super::super::state::{MatchState, Sides};
use crate::reconciliation::scoring::normalized_reference;

/// Match on reference numbers.
///
/// A reference is only acted on when it identifies exactly one available counterpart; a
/// reference reused across several statement rows is left for the later passes. Dates are
/// deliberately ignored here, so settlement lag on a row carrying a UTR still matches outright.
pub(in crate::reconciliation::engine) fn match_by_reference(sides: &Sides, state: &mut MatchState) {
    let by_reference = index_external_references(sides);

    for &ledger_index in &sides.ledger_order {
        if let Some(external_index) = unique_counterpart(sides, state, &by_reference, ledger_index)
        {
            state.record_exact_match(sides, ledger_index, external_index);
        }
    }
}

/// External indices grouped by normalized reference
fn index_external_references(sides: &Sides) -> HashMap<String, Vec<usize>> {
    sides
        .external
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            normalized_reference(t.reference.as_deref()).map(|key| (key, index))
        })
        .fold(HashMap::new(), |mut by_reference, (key, index)| {
            by_reference.entry(key).or_insert_with(Vec::new).push(index);
            by_reference
        })
}

/// The single free external row sharing this ledger row's reference, amount and direction
fn unique_counterpart(
    sides: &Sides,
    state: &MatchState,
    by_reference: &HashMap<String, Vec<usize>>,
    ledger_index: usize,
) -> Option<usize> {
    let ledger = &sides.ledger[ledger_index];
    let key = normalized_reference(ledger.reference.as_deref())?;

    let mut available = by_reference.get(&key)?.iter().copied().filter(|&index| {
        let external = &sides.external[index];
        state.is_external_free(index)
            && external.amount == ledger.amount
            && external.entry_type == ledger.entry_type
    });

    let first = available.next()?;
    // Ambiguous - let the later passes decide
    available.next().is_none().then_some(first)
}
