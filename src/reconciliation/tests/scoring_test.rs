use crate::reconciliation::config::ReconciliationConfig;
use crate::reconciliation::scoring::*;
use crate::reconciliation::types::{ExternalSource, ExternalTransaction, LedgerTransaction};
use crate::types::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn pair(ledger_amount: i32, external_amount: i32) -> (LedgerTransaction, ExternalTransaction) {
    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    let ledger = LedgerTransaction::new(
        "l",
        date,
        BigDecimal::from(ledger_amount),
        "Rent",
        EntryType::Debit,
        "bank",
    );
    let external = ExternalTransaction::new(
        "e",
        date,
        BigDecimal::from(external_amount),
        "Rent",
        EntryType::Debit,
        ExternalSource::Upi {
            provider: "x".to_string(),
        },
    );
    (ledger, external)
}

#[test]
fn test_identical_pair_scores_one() {
    let (ledger, external) = pair(100, 100);
    let (score, differences) = score_pair(&ReconciliationConfig::default(), &ledger, &external);
    assert!((score - EXACT_MATCH_SCORE).abs() < f64::EPSILON);
    assert!(differences.is_empty());
}

#[test]
fn test_zero_amounts_earn_no_amount_credit_when_they_differ() {
    let (ledger, external) = pair(0, 5);
    let config = ReconciliationConfig::default();
    let dimension = score_amount(&config, &ledger, &external);
    assert!(dimension.earned.abs() < f64::EPSILON);
    assert!(dimension.difference.is_some());
    assert_eq!(ledger.amount, BigDecimal::from(0));
}

#[test]
fn test_weights_come_from_config() {
    let (ledger, mut external) = pair(100, 100);
    external.entry_type = EntryType::Credit;
    let mut config = ReconciliationConfig::default();
    let (default_score, _) = score_pair(&config, &ledger, &external);

    config.weights.entry_type = 0.0;
    let (ignoring_direction, _) = score_pair(&config, &ledger, &external);

    assert!(ignoring_direction > default_score);
}

#[test]
fn test_normalized_reference() {
    assert_eq!(normalized_reference(Some(" utr-1 ")), Some("UTR-1".into()));
    assert_eq!(normalized_reference(Some("   ")), None);
    assert_eq!(normalized_reference(None), None);
}
