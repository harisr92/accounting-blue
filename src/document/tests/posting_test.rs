//! Tests for posting breakdowns to the ledger

use crate::document::posting::*;
use crate::document::tax::{TaxBreakdown, TaxComponent};
use crate::types::EntryType;
use bigdecimal::BigDecimal;

fn vat_breakdown(base: i32, vat: i32) -> TaxBreakdown {
    TaxBreakdown::new(
        BigDecimal::from(base),
        vec![TaxComponent::new("VAT", BigDecimal::from(vat))],
    )
}

fn accounts() -> PostingAccounts {
    PostingAccounts::new("ar", "sales").with_tax("VAT", "vat_out")
}

#[test]
fn test_legs_are_receivable_sales_then_each_tax() {
    assert_eq!(
        posting_legs(&vat_breakdown(100, 20)),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(120)),
            (PostingLeg::Sales, BigDecimal::from(100)),
            (PostingLeg::tax("VAT"), BigDecimal::from(20)),
        ]
    );
}

#[test]
fn test_zero_legs_are_left_out() {
    let legs = posting_legs(&vat_breakdown(100, 0));
    assert_eq!(legs.len(), 2);
    assert!(posting_legs(&TaxBreakdown::default()).is_empty());
}

#[test]
fn test_only_the_receivable_is_a_debit() {
    assert_eq!(PostingLeg::Receivable.side(), EntryType::Debit);
    assert_eq!(PostingLeg::Sales.side(), EntryType::Credit);
    assert_eq!(PostingLeg::tax("VAT").side(), EntryType::Credit);
}

#[test]
fn test_tax_legs_read_as_the_code_and_output() {
    assert_eq!(PostingLeg::tax("VAT").to_string(), "VAT output");
    assert_eq!(PostingLeg::Receivable.to_string(), "Receivable");
}

#[test]
fn test_entries_post_to_the_named_accounts_and_balance() {
    let entries =
        breakdown_entries(&vat_breakdown(100, 20), &accounts(), PostingLeg::side).unwrap();
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
fn test_reversed_sides_post_a_credit_note() {
    let entries = breakdown_entries(&vat_breakdown(100, 20), &accounts(), |leg| {
        leg.side().opposite()
    })
    .unwrap();
    assert_eq!(entries[0].entry_type, EntryType::Credit);
    assert_eq!(entries[1].entry_type, EntryType::Debit);
}

#[test]
fn test_a_tax_without_an_account_is_refused() {
    let accounts = PostingAccounts::new("ar", "sales");
    assert_eq!(
        breakdown_entries(&vat_breakdown(100, 20), &accounts, PostingLeg::side),
        Err(PostingError::NoTaxAccount {
            code: "VAT".to_string()
        })
    );
}

#[test]
fn test_a_zero_breakdown_has_nothing_to_post() {
    assert_eq!(
        breakdown_entries(&TaxBreakdown::default(), &accounts(), PostingLeg::side),
        Err(PostingError::NothingToPost)
    );
}
