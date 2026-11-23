//! Tests for the checks every document must pass

use crate::document::discount::{Discount, DiscountError};
use crate::document::invoice::Invoice;
use crate::document::pricing::{LineError, LineItem};
use crate::document::tax::FlatRate;
use crate::document::validation::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 3, day).unwrap()
}

fn line(price: i32) -> LineItem {
    LineItem::new(
        "Chair",
        BigDecimal::from(1),
        BigDecimal::from(price),
        BigDecimal::from(20),
    )
    .unwrap()
}

fn invoice(lines: Vec<LineItem>) -> Invoice<FlatRate> {
    Invoice::new("INV-001", date(10), "Acme", lines, FlatRate::new("VAT")).unwrap()
}

#[test]
fn test_a_valid_invoice_has_no_issues() {
    let report = validate_document(&invoice(vec![line(50)]), date(10));
    assert!(report.issues().is_empty());
    assert!(report.is_compliant());
}

#[test]
fn test_edited_fields_are_caught_after_construction() {
    let mut doc = invoice(vec![line(50)]);
    doc.lines[0].description = " ".to_string();
    doc.discount = Some(Discount::Amount(BigDecimal::from(100)));
    let issues = validate_document(&doc, date(10)).into_issues();
    assert_eq!(
        issues,
        vec![
            ValidationIssue::InvalidLine {
                line: 0,
                reason: LineError::EmptyDescription
            },
            ValidationIssue::InvalidDiscount(DiscountError::ExceedsValue {
                discount: BigDecimal::from(100),
                value: BigDecimal::from(50).with_scale(2),
            }),
        ]
    );
}

#[test]
fn test_no_lines_is_an_error() {
    let mut doc = invoice(vec![line(50)]);
    doc.lines.clear();
    let report = validate_document(&doc, date(10));
    assert_eq!(report.issues(), &[ValidationIssue::NoLines]);
    assert!(!report.is_compliant());
}

#[test]
fn test_a_document_dated_after_the_check_is_an_error_but_the_same_day_is_not() {
    assert!(validate_document(&invoice(vec![line(50)]), date(10)).is_compliant());
    let report = validate_document(&invoice(vec![line(50)]), date(9));
    assert_eq!(
        report.errors().collect::<Vec<_>>(),
        vec![&ValidationIssue::FutureDate {
            date: date(10),
            as_of: date(9)
        }]
    );
}

#[test]
fn test_a_zero_value_line_is_only_a_warning() {
    let report = validate_document(&invoice(vec![line(50), line(0)]), date(10));
    assert_eq!(
        report.warnings().collect::<Vec<_>>(),
        vec![&ValidationIssue::ZeroValueLine { line: 1 }]
    );
    assert!(report.is_compliant());
}

#[test]
fn test_issue_messages_count_lines_from_one() {
    let issue = ValidationIssue::ZeroValueLine { line: 1 };
    assert_eq!(issue.to_string(), "line 2: taxable value rounds to zero");
    assert_eq!(issue.severity(), Severity::Warning);
}
