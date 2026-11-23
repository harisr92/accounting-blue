//! Tests for posting legs and turning them into entries

use crate::document::posting::*;
use crate::types::EntryType;
use bigdecimal::BigDecimal;

fn legs() -> Vec<(PostingLeg, BigDecimal)> {
    vec![
        (PostingLeg::Receivable, BigDecimal::from(120)),
        (PostingLeg::Revenue, BigDecimal::from(100)),
        (PostingLeg::output_tax("VAT"), BigDecimal::from(20)),
    ]
}

fn account_for(leg: &PostingLeg) -> Option<&'static str> {
    match leg {
        PostingLeg::Receivable => Some("ar"),
        PostingLeg::Revenue => Some("sales"),
        PostingLeg::OutputTax(code) if code == "VAT" => Some("vat_out"),
        PostingLeg::OutputTax(_) => None,
    }
}

#[test]
fn test_only_the_receivable_is_a_debit() {
    assert_eq!(PostingLeg::Receivable.side(), EntryType::Debit);
    assert_eq!(PostingLeg::Revenue.side(), EntryType::Credit);
    assert_eq!(PostingLeg::output_tax("VAT").side(), EntryType::Credit);
}

#[test]
fn test_legs_read_as_their_entry_memos() {
    assert_eq!(PostingLeg::Receivable.to_string(), "Receivable");
    assert_eq!(PostingLeg::Revenue.to_string(), "Sales");
    assert_eq!(PostingLeg::output_tax("VAT").to_string(), "VAT output");
}

#[test]
fn test_each_leg_becomes_an_entry_on_its_account_with_its_memo() {
    let entries = legs_to_entries(legs(), account_for, PostingLeg::side).unwrap();
    let posted: Vec<_> = entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.entry_type, e.amount.clone()))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("ar", EntryType::Debit, BigDecimal::from(120)),
            ("sales", EntryType::Credit, BigDecimal::from(100)),
            ("vat_out", EntryType::Credit, BigDecimal::from(20)),
        ]
    );
    assert_eq!(entries[2].description.as_deref(), Some("VAT output"));
}

#[test]
fn test_a_tax_without_an_account_is_refused() {
    let legs = vec![(PostingLeg::output_tax("GST"), BigDecimal::from(1))];
    assert_eq!(
        legs_to_entries(legs, account_for, PostingLeg::side),
        Err(PostingError::NoTaxAccount {
            code: "GST".to_string()
        })
    );
}

#[test]
fn test_no_legs_is_nothing_to_post() {
    assert_eq!(
        legs_to_entries(Vec::new(), account_for, PostingLeg::side),
        Err(PostingError::NothingToPost)
    );
}
