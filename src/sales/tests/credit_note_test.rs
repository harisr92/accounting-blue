//! Tests for credit notes against invoices

use crate::document::error::BillingError;
use crate::document::line::LineItem;
use crate::document::posting::PostingAccounts;
use crate::document::pricing::Document;
use crate::document::tax::FlatRate;
use crate::sales::credit_note::*;
use crate::sales::invoice::Invoice;
use crate::types::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 3, day).unwrap()
}

fn chairs(quantity: i32) -> LineItem {
    LineItem::new(
        "Chair",
        BigDecimal::from(quantity),
        BigDecimal::from(50),
        BigDecimal::from(20),
    )
    .unwrap()
}

fn invoice() -> Invoice<FlatRate> {
    Invoice::new(
        "INV-001",
        date(10),
        "Acme Ltd",
        vec![chairs(10)],
        FlatRate::new("VAT"),
    )
    .unwrap()
}

#[test]
fn test_note_records_its_invoice_and_takes_its_customer_and_regime() {
    let note = CreditNote::new(&invoice(), "CN-001", date(12), vec![chairs(2)]).unwrap();
    assert_eq!(
        note.original,
        OriginalDocument {
            number: "INV-001".to_string(),
            date: date(10),
            value: BigDecimal::from(600),
        }
    );
    assert_eq!(note.customer, "Acme Ltd");
    assert_eq!(note.regime, FlatRate::new("VAT"));
    assert_eq!(note.tax_breakdown().unwrap().total, BigDecimal::from(120));
}

#[test]
fn test_note_may_be_dated_the_same_day_but_not_before_its_invoice() {
    assert!(CreditNote::new(&invoice(), "CN-1", date(10), vec![chairs(1)]).is_ok());
    assert!(matches!(
        CreditNote::new(&invoice(), "CN-1", date(9), vec![chairs(1)]),
        Err(BillingError::NoteBeforeOriginal { .. })
    ));
}

#[test]
fn test_note_may_credit_the_whole_invoice_but_no_more() {
    assert!(CreditNote::new(&invoice(), "CN-1", date(12), vec![chairs(10)]).is_ok());
    assert!(matches!(
        CreditNote::new(&invoice(), "CN-1", date(12), vec![chairs(11)]),
        Err(BillingError::CreditExceedsInvoice { .. })
    ));
}

#[test]
fn test_note_needs_a_number_and_lines() {
    assert!(matches!(
        CreditNote::new(&invoice(), "", date(12), vec![chairs(1)]),
        Err(BillingError::EmptyNumber)
    ));
    assert!(matches!(
        CreditNote::new(&invoice(), "CN-1", date(12), vec![]),
        Err(BillingError::NoLines)
    ));
}

#[test]
fn test_note_posts_the_reverse_of_an_invoice() {
    let note = CreditNote::new(&invoice(), "CN-001", date(12), vec![chairs(2)]).unwrap();
    let accounts = PostingAccounts::new("ar", "returns").with_tax("VAT", "vat_out");
    let posted: Vec<_> = note
        .to_entries(&accounts)
        .unwrap()
        .iter()
        .map(|e| (e.account_id.clone(), e.entry_type, e.amount.clone()))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("ar".to_string(), EntryType::Credit, BigDecimal::from(120)),
            (
                "returns".to_string(),
                EntryType::Debit,
                BigDecimal::from(100)
            ),
            (
                "vat_out".to_string(),
                EntryType::Debit,
                BigDecimal::from(20)
            ),
        ]
    );
}
