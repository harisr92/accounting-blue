use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::engine::*;
use crate::reconciliation::types::{
    ExternalSource, ExternalTransaction, LedgerTransaction, MatchDifference, ReconciliationReport,
    ReconciliationStatus,
};
use crate::types::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
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
        &[ledger_txn("txn-1", 15, "1000", "Payment from customer")],
        &[external_txn("ext-1", 15, "1000", "Payment from customer")],
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
        &[ledger_txn("txn-1", 15, "1000", "Rent")],
        &[external_txn("ext-1", 15, "1000.00", "Rent")],
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

    let report = ReconciliationEngine::default().reconcile(&ledger, &external, source());

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

    let report = ReconciliationEngine::default().reconcile(&ledger, &external, source());

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
        &[ledger_txn("txn-1", 15, "1000", "Payment from customer")],
        &[external_txn("ext-1", 16, "1000", "Payment from customer")],
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
        &[ledger_txn("txn-1", 15, "1000", "Payment from customer")],
        &[external_txn("ext-1", 25, "1000", "Payment from customer")],
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
        &[ledger_txn("txn-1", 1, "1000", "Payment from customer")],
        &[ExternalTransaction::new(
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
        &[ledger_txn("txn-1", 15, "1000", "Payment from customer")],
        &[external_txn(
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
        &[ledger_txn("txn-1", 15, "1000", "Payment from customer")],
        &[external],
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
        &[
            ledger_txn("txn-0", 15, "0", "Nil adjustment"),
            ledger_txn("txn-1", 15, "0", "Opening float"),
        ],
        &[
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
    let forward = engine.reconcile(&ledger, &external, source());

    let reversed_ledger: Vec<_> = ledger.into_iter().rev().collect();
    let reversed_external: Vec<_> = external.into_iter().rev().collect();
    let backward = engine.reconcile(&reversed_ledger, &reversed_external, source());

    assert_eq!(
        forward.reconciliation_items, backward.reconciliation_items,
        "reconciliation must not depend on input ordering"
    );
    assert_eq!(forward.summary, backward.summary);
}

#[test]
fn test_duplicate_externals_are_matched_only_once() {
    let report = ReconciliationEngine::default().reconcile(
        &[ledger_txn("txn-1", 15, "1000", "Payment")],
        &[
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
    let report = ReconciliationEngine::default().reconcile(&[], &[], source());

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
        &[ledger_credit, ledger_debit],
        &[external_credit],
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
        &[ledger_txn("txn-1", 15, "1000", "acme ltd payment")],
        &external,
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

    let report = ReconciliationEngine::default().reconcile(&ledger, &external, source());

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
