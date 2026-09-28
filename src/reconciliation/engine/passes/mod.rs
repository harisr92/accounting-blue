//! The matching passes, in the order [`ReconciliationEngine::reconcile`] runs them
//!
//! [`ReconciliationEngine::reconcile`]: super::ReconciliationEngine::reconcile

mod exact;
mod reference;
mod scored;
mod suggestions;

pub(super) use exact::match_exactly;
pub(super) use reference::match_by_reference;
pub(super) use scored::{assign_best_first, score_remaining};
pub(super) use suggestions::collect_unmatched;
