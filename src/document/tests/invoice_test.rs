//! Tests for invoices under a tax regime

use crate::document::discount::Discount;
use crate::document::error::BillingError;
use crate::document::invoice::*;
use crate::document::posting::{PostingAccounts, PostingError};
use crate::document::pricing::{LineItem, TaxDocument};
use crate::document::tax::{FlatRate, NoTax};
use crate::types::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 3, 1).unwrap()
}

fn line(quantity: i32, price: i32, rate: i32) -> LineItem {
    LineItem::new(
        "Item",
        BigDecimal::from(quantity),
        BigDecimal::from(price),
        BigDecimal::from(rate),
    )
    .unwrap()
}

fn vat_invoice(lines: Vec<LineItem>) -> Invoice<FlatRate> {
    Invoice::new("INV-001", date(), "Acme Ltd", lines, FlatRate::new("VAT")).unwrap()
}

fn accounts() -> PostingAccounts {
    PostingAccounts::new("ar", "sales").with_tax("VAT", "vat_out")
}

#[test]
fn test_invoice_needs_a_number_and_lines() {
    let vat = FlatRate::new("VAT");
    assert!(matches!(
        Invoice::new("  ", date(), "Acme", vec![line(1, 1, 0)], vat.clone()),
        Err(BillingError::EmptyNumber)
    ));
    assert!(matches!(
        Invoice::new("INV-1", date(), "Acme", vec![], vat),
        Err(BillingError::NoLines)
    ));
}

#[test]
fn test_invoice_number_is_trimmed() {
    let invoice = Invoice::new(" INV-9 ", date(), "Acme", vec![line(1, 1, 0)], NoTax).unwrap();
    assert_eq!(invoice.number, "INV-9");
}

#[test]
fn test_lines_at_different_rates_are_taxed_separately() {
    let invoice = vat_invoice(vec![line(2, 100, 20), line(1, 100, 5)]);
    let breakdown = invoice.tax_breakdown().unwrap();
    assert_eq!(breakdown.taxable_value, BigDecimal::from(300));
    assert_eq!(breakdown.tax("VAT"), BigDecimal::from(45));
    assert_eq!(breakdown.total, BigDecimal::from(345));
}

#[test]
fn test_invoice_discount_comes_off_before_tax() {
    let invoice = vat_invoice(vec![line(1, 1000, 20)])
        .with_discount(Discount::Percent(BigDecimal::from(10)))
        .unwrap();
    let breakdown = invoice.tax_breakdown().unwrap();
    assert_eq!(breakdown.taxable_value, BigDecimal::from(900));
    assert_eq!(breakdown.total, BigDecimal::from(1080));
}

#[test]
fn test_discount_larger_than_the_subtotal_is_refused() {
    assert!(matches!(
        vat_invoice(vec![line(1, 10, 20)]).with_discount(Discount::Amount(BigDecimal::from(11))),
        Err(BillingError::InvalidDiscount(_))
    ));
}

#[test]
fn test_untaxed_invoice_totals_its_lines() {
    let invoice = Invoice::new("INV-2", date(), "Acme", vec![line(3, 7, 20)], NoTax).unwrap();
    let breakdown = invoice.tax_breakdown().unwrap();
    assert!(breakdown.components.is_empty());
    assert_eq!(breakdown.total, BigDecimal::from(21));
}

#[test]
fn test_invoice_posts_balanced_entries() {
    let entries = vat_invoice(vec![line(1, 100, 20)])
        .to_entries(&accounts())
        .unwrap();
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
fn test_posting_a_tax_without_an_account_is_refused() {
    let result = vat_invoice(vec![line(1, 100, 20)]).to_entries(&PostingAccounts::new("ar", "s"));
    assert!(matches!(
        result,
        Err(BillingError::Posting(PostingError::NoTaxAccount { .. }))
    ));
}

#[test]
fn test_invoice_round_trips_through_json_and_validates_on_the_way_in() {
    let invoice = vat_invoice(vec![line(1, 100, 20)])
        .with_discount(Discount::Amount(BigDecimal::from(10)))
        .unwrap();
    let json = serde_json::to_string(&invoice).unwrap();
    let back: Invoice<FlatRate> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, invoice);

    let bad = json.replace("INV-001", " ");
    assert!(serde_json::from_str::<Invoice<FlatRate>>(&bad).is_err());
}
