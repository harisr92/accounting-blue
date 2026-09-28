use crate::reconciliation::types::*;
use crate::types::{Entry, EntryType, Transaction};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use uuid::Uuid;

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 11, 15).unwrap()
}

fn transaction() -> Transaction {
    let mut transaction = Transaction::new(
        "txn-1".to_string(),
        date(),
        "Customer payment".to_string(),
        Some("INV-9".to_string()),
    );
    transaction.add_entry(Entry::debit("bank", BigDecimal::from(1000), None));
    transaction.add_entry(Entry::credit("sales", BigDecimal::from(1000), None));
    transaction
}

#[test]
fn test_from_transaction_projects_the_debit_leg() {
    let leg = LedgerTransaction::from_transaction(&transaction(), "bank").unwrap();

    assert_eq!(leg.id, "txn-1");
    assert_eq!(leg.date, date());
    assert_eq!(leg.amount, BigDecimal::from(1000));
    assert_eq!(leg.entry_type, EntryType::Debit);
    assert_eq!(leg.account_id, "bank");
    assert_eq!(leg.reference.as_deref(), Some("INV-9"));
}

#[test]
fn test_from_transaction_projects_the_credit_leg() {
    let leg = LedgerTransaction::from_transaction(&transaction(), "sales").unwrap();

    assert_eq!(leg.amount, BigDecimal::from(1000));
    assert_eq!(leg.entry_type, EntryType::Credit);
}

#[test]
fn test_from_transaction_nets_repeated_entries() {
    let mut txn = Transaction::new(
        "txn-2".to_string(),
        date(),
        "Split settlement".to_string(),
        None,
    );
    txn.add_entry(Entry::debit("bank", BigDecimal::from(1000), None));
    txn.add_entry(Entry::credit("bank", BigDecimal::from(150), None));
    txn.add_entry(Entry::credit("sales", BigDecimal::from(850), None));

    let leg = LedgerTransaction::from_transaction(&txn, "bank").unwrap();
    assert_eq!(leg.amount, BigDecimal::from(850));
    assert_eq!(leg.entry_type, EntryType::Debit);
}

#[test]
fn test_from_transaction_without_the_account() {
    assert!(LedgerTransaction::from_transaction(&transaction(), "petty-cash").is_none());
}

#[test]
fn test_from_transaction_with_a_leg_that_nets_to_zero() {
    let mut txn = Transaction::new("txn-3".to_string(), date(), "Contra".to_string(), None);
    txn.add_entry(Entry::debit("bank", BigDecimal::from(500), None));
    txn.add_entry(Entry::credit("bank", BigDecimal::from(500), None));

    let leg = LedgerTransaction::from_transaction(&txn, "bank").unwrap();
    assert_eq!(leg.amount, BigDecimal::from(0));
    assert_eq!(leg.entry_type, EntryType::Debit);
}

#[test]
fn test_needs_review_skips_full_matches() {
    let items = vec![
        ReconciliationStatus::Matched {
            ledger_id: "txn-1".to_string(),
            external_id: "ext-1".to_string(),
            match_score: 1.0,
        },
        ReconciliationStatus::UnmatchedLedger {
            ledger_id: "txn-2".to_string(),
            possible_matches: Vec::new(),
        },
    ];

    let report = ReconciliationReport {
        id: Uuid::new_v4(),
        created_at: chrono::Utc::now().naive_utc(),
        period_start: date(),
        period_end: date(),
        external_source: ExternalSource::Upi {
            provider: "Test".to_string(),
        },
        account_ids: vec!["bank".to_string()],
        total_ledger_transactions: 2,
        total_external_transactions: 1,
        matched_count: 1,
        unmatched_ledger_count: 1,
        unmatched_external_count: 0,
        partial_match_count: 0,
        reconciliation_items: items,
        summary: ReconciliationSummary {
            ledger_balance: BigDecimal::from(0),
            external_balance: BigDecimal::from(0),
            difference: BigDecimal::from(0),
            match_rate: 0.5,
            confidence_score: 0.75,
        },
    };

    assert_eq!(report.needs_review().count(), 1);
    assert!(!report.is_fully_reconciled());
}

#[test]
fn test_differences_render_for_humans() {
    let difference = MatchDifference::AmountDifference {
        ledger_amount: BigDecimal::from(4300),
        external_amount: BigDecimal::from(4299),
        difference: BigDecimal::from(1),
    };
    assert_eq!(
        difference.to_string(),
        "amount 4300 in the ledger but 4299 externally, off by 1"
    );

    let missing = MatchDifference::ReferenceMismatch {
        ledger_reference: Some("UTR-1".to_string()),
        external_reference: None,
    };
    assert_eq!(
        missing.to_string(),
        "reference UTR-1 in the ledger but (none) externally"
    );
}
