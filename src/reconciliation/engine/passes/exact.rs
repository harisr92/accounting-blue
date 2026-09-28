//! Pass 2: identical date, amount and direction, found by index rather than a nested scan

use std::collections::{HashMap, VecDeque};

use bigdecimal::BigDecimal;
use chrono::NaiveDate;

use super::super::state::{MatchState, Sides};
use crate::types::EntryType;

/// Key identifying transactions that agree exactly, with the amount rendered in canonical form so
/// that `1.0` and `1.00` land in the same bucket
type ExactKey = (NaiveDate, String, EntryType);

fn exact_key(date: NaiveDate, amount: &BigDecimal, entry_type: EntryType) -> ExactKey {
    (date, amount.normalized().to_string(), entry_type)
}

/// Pair each free ledger row with the first free external row, in id order, that agrees exactly
pub(in crate::reconciliation::engine) fn match_exactly(sides: &Sides, state: &mut MatchState) {
    let mut by_key: HashMap<ExactKey, VecDeque<usize>> =
        sides
            .free_external(state)
            .fold(HashMap::new(), |mut by_key, index| {
                let t = &sides.external[index];
                by_key
                    .entry(exact_key(t.date, &t.amount, t.entry_type))
                    .or_insert_with(VecDeque::new)
                    .push_back(index);
                by_key
            });

    let free_ledger: Vec<usize> = sides.free_ledger(state).collect();
    for ledger_index in free_ledger {
        let t = &sides.ledger[ledger_index];
        let counterpart = by_key
            .get_mut(&exact_key(t.date, &t.amount, t.entry_type))
            .and_then(VecDeque::pop_front);

        if let Some(external_index) = counterpart {
            state.record_exact_match(sides, ledger_index, external_index);
        }
    }
}
