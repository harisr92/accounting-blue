//! Tests for posting sales documents

use crate::document::posting::{PostingError, PostingLeg};
use crate::document::tax::{TaxBreakdown, TaxComponent};
use crate::sales::posting::*;
use crate::types::EntryType;
use bigdecimal::BigDecimal;

fn vat_breakdown(base: i32, vat: i32) -> TaxBreakdown {
    TaxBreakdown::new(
        BigDecimal::from(base),
        vec![TaxComponent::new("VAT", BigDecimal::from(vat))],
    )
}

fn accounts() -> SalesAccounts {
    SalesAccounts::new("ar", "sales").with_output_tax("VAT", "vat_out")
}

#[test]
fn test_legs_are_receivable_revenue_then_each_output_tax() {
    assert_eq!(
        sales_legs(&vat_breakdown(100, 20)),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(120)),
            (PostingLeg::Revenue, BigDecimal::from(100)),
            (PostingLeg::output_tax("VAT"), BigDecimal::from(20)),
        ]
    );
}

#[test]
fn test_zero_legs_are_left_out() {
    assert_eq!(sales_legs(&vat_breakdown(100, 0)).len(), 2);
    assert!(sales_legs(&TaxBreakdown::default()).is_empty());
}

#[test]
fn test_accounts_name_one_account_per_leg() {
    let accounts = accounts();
    assert_eq!(accounts.account_for(&PostingLeg::Receivable), Some("ar"));
    assert_eq!(accounts.account_for(&PostingLeg::Revenue), Some("sales"));
    assert_eq!(
        accounts.account_for(&PostingLeg::output_tax("VAT")),
        Some("vat_out")
    );
    assert_eq!(accounts.account_for(&PostingLeg::output_tax("GST")), None);
}

#[test]
fn test_invoice_entries_debit_the_receivable_and_balance() {
    let entries = sales_entries(&vat_breakdown(100, 20), &accounts(), PostingLeg::side).unwrap();
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
}

#[test]
fn test_reversed_sides_post_a_credit_note() {
    let entries = sales_entries(&vat_breakdown(100, 20), &accounts(), |leg| {
        leg.side().opposite()
    })
    .unwrap();
    assert_eq!(entries[0].entry_type, EntryType::Credit);
    assert_eq!(entries[1].entry_type, EntryType::Debit);
}

#[test]
fn test_an_output_tax_without_an_account_is_refused() {
    assert_eq!(
        sales_entries(
            &vat_breakdown(100, 20),
            &SalesAccounts::new("ar", "sales"),
            PostingLeg::side
        ),
        Err(PostingError::NoTaxAccount {
            code: "VAT".to_string()
        })
    );
}

#[test]
fn test_a_zero_breakdown_has_nothing_to_post() {
    assert_eq!(
        sales_entries(&TaxBreakdown::default(), &accounts(), PostingLeg::side),
        Err(PostingError::NothingToPost)
    );
}
