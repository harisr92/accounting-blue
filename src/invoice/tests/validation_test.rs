use crate::invoice::hsn_lookup::HsnMaster;
use crate::invoice::types::{GstInvoice, GstLineItem, Gstin, Recipient, StateCode};
use crate::invoice::types::{InvoiceNumberError, LineItemError};
use crate::invoice::validation::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

/// A day in November 2025, after the master's rate schedule took effect
fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 11, day).unwrap()
}

/// A line of two units at 500 each, charged at `rate`
fn item(hsn_sac: &str, rate: i32) -> GstLineItem {
    GstLineItem::new(
        hsn_sac,
        "Line",
        BigDecimal::from(2),
        BigDecimal::from(500),
        BigDecimal::from(rate),
    )
    .unwrap()
}

fn invoice(buyer: &str, line_items: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        "INV/2025-26/001",
        date(15),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(buyer).unwrap(),
        line_items,
    )
    .unwrap()
}

/// An intra-state invoice for IT consulting (SAC 998314, 18% by default) and glass (HSN 7010,
/// 18% by default) that breaks no rule
fn clean_invoice() -> GstInvoice {
    invoice(BUYER_SAME_STATE, vec![item("998314", 18), item("7010", 18)])
}

fn check(invoice: &GstInvoice) -> InvoiceValidationReport {
    validate_invoice(invoice, date(30), HsnMaster::global())
}

#[test]
fn test_clean_invoice_is_compliant() {
    let report = check(&clean_invoice());
    assert!(report.is_compliant());
    assert!(report.issues().is_empty());

    let inter_state = invoice(BUYER_OTHER_STATE, vec![item("998314", 18)]);
    assert!(check(&inter_state).issues().is_empty());
}

#[test]
fn test_future_dated_invoice_is_an_error() {
    let invoice = clean_invoice();

    let report = validate_invoice(&invoice, date(14), HsnMaster::global());
    assert_eq!(
        report.issues(),
        [ComplianceIssue::FutureDate {
            invoice_date: date(15),
            as_of: date(14),
        }]
    );
    assert!(!report.is_compliant());

    assert!(validate_invoice(&invoice, date(15), HsnMaster::global()).is_compliant());
}

#[test]
fn test_invoice_number_edited_after_construction_is_an_error() {
    let mut invoice = clean_invoice();

    invoice.invoice_number = String::new();
    assert_eq!(
        check(&invoice).issues(),
        [ComplianceIssue::InvalidInvoiceNumber {
            value: String::new(),
            reason: InvoiceNumberError::Length,
        }]
    );

    invoice.invoice_number = "INV 001".to_string();
    assert!(matches!(
        check(&invoice).issues(),
        [ComplianceIssue::InvalidInvoiceNumber {
            reason: InvoiceNumberError::Characters,
            ..
        }]
    ));
}

#[test]
fn test_invoice_without_line_items_is_an_error() {
    let mut invoice = clean_invoice();
    invoice.line_items.clear();

    assert_eq!(check(&invoice).issues(), [ComplianceIssue::NoLineItems]);
}

#[test]
fn test_missing_or_malformed_hsn_sac_is_an_error() {
    let mut invoice = clean_invoice();
    invoice.line_items[0].hsn_sac = String::new();
    invoice.line_items[1].hsn_sac = "70A0".to_string();

    assert_eq!(
        check(&invoice).issues(),
        [
            ComplianceIssue::InvalidHsnSac {
                line: 0,
                code: String::new(),
            },
            ComplianceIssue::InvalidHsnSac {
                line: 1,
                code: "70A0".to_string(),
            },
        ]
    );
}

#[test]
fn test_malformed_hsn_sac_does_not_hide_other_line_errors() {
    let mut invoice = clean_invoice();
    invoice.line_items[0].hsn_sac = "70A0".to_string();
    invoice.line_items[0].unit_price = BigDecimal::from(-500);

    assert_eq!(
        check(&invoice).issues(),
        [
            ComplianceIssue::InvalidHsnSac {
                line: 0,
                code: "70A0".to_string(),
            },
            ComplianceIssue::InvalidLineItem {
                line: 0,
                reason: LineItemError::NegativeUnitPrice,
            },
        ]
    );
}

#[test]
fn test_negative_and_zero_amounts_are_errors() {
    let mut invoice = clean_invoice();
    invoice.line_items[0].unit_price = BigDecimal::from(-500);
    invoice.line_items[1].quantity = BigDecimal::from(0);

    let report = check(&invoice);
    let errors: Vec<_> = report.errors().collect();
    assert_eq!(
        errors,
        [
            &ComplianceIssue::InvalidLineItem {
                line: 0,
                reason: LineItemError::NegativeUnitPrice,
            },
            &ComplianceIssue::InvalidLineItem {
                line: 1,
                reason: LineItemError::NonPositiveQuantity,
            },
        ]
    );
}

#[test]
fn test_missing_description_and_out_of_range_rate_are_errors() {
    let mut invoice = clean_invoice();
    invoice.line_items[0].description = "  ".to_string();
    invoice.line_items[1].gst_rate = BigDecimal::from(150);

    let report = check(&invoice);
    let errors: Vec<_> = report.errors().collect();
    assert_eq!(
        errors,
        [
            &ComplianceIssue::InvalidLineItem {
                line: 0,
                reason: LineItemError::EmptyDescription,
            },
            &ComplianceIssue::InvalidLineItem {
                line: 1,
                reason: LineItemError::RateOutOfRange(BigDecimal::from(150)),
            },
        ]
    );
}

#[test]
fn test_same_seller_and_buyer_is_an_error() {
    let invoice = invoice(SELLER, vec![item("998314", 18)]);

    assert_eq!(
        check(&invoice).issues(),
        [ComplianceIssue::SameSellerAndBuyer(SELLER.to_string())]
    );
}

#[test]
fn test_rate_differing_from_hsn_default_is_a_warning() {
    // Printed books (HSN 4901) are 0% by default
    let invoice = invoice(BUYER_SAME_STATE, vec![item("4901", 5)]);

    let report = check(&invoice);
    assert!(report.is_compliant());
    assert_eq!(
        report.issues(),
        [ComplianceIssue::RateDiffersFromHsnDefault {
            line: 0,
            code: "4901".to_string(),
            rate: BigDecimal::from(5),
            default: BigDecimal::from(0),
        }]
    );
    assert_eq!(report.issues()[0].severity(), Severity::Warning);
}

#[test]
fn test_rate_is_not_compared_before_the_schedule_took_effect() {
    let mut invoice = invoice(BUYER_SAME_STATE, vec![item("4901", 5)]);
    invoice.invoice_date = HsnMaster::global().effective_from().pred_opt().unwrap();

    assert!(check(&invoice).issues().is_empty());
}

#[test]
fn test_hsn_default_matches_after_heading_fallback() {
    // 84713010 falls back to 847130, which is 18% by default
    let invoice = invoice(BUYER_SAME_STATE, vec![item("84713010", 18)]);
    assert!(check(&invoice).issues().is_empty());
}

#[test]
fn test_unknown_hsn_sac_is_a_warning() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("0000", 18)]);

    let report = check(&invoice);
    assert!(report.is_compliant());
    assert_eq!(
        report.warnings().collect::<Vec<_>>(),
        [&ComplianceIssue::UnknownHsnSac {
            line: 0,
            code: "0000".to_string(),
        }]
    );
}

#[test]
fn test_zero_value_line_is_a_warning() {
    let mut invoice = clean_invoice();
    invoice.line_items[1].unit_price = BigDecimal::from(0);

    let report = check(&invoice);
    assert!(report.is_compliant());
    assert_eq!(
        report.into_issues(),
        vec![ComplianceIssue::ZeroValueLine { line: 1 }]
    );
}

#[test]
fn test_every_issue_is_reported_not_just_the_first() {
    let mut invoice = invoice(SELLER, vec![item("4901", 5), item("998314", 18)]);
    invoice.invoice_number = "INV#1".to_string();
    invoice.line_items[1].quantity = BigDecimal::from(-1);

    let report = validate_invoice(&invoice, date(1), HsnMaster::global());
    let severities: Vec<_> = report
        .issues()
        .iter()
        .map(ComplianceIssue::severity)
        .collect();
    assert_eq!(
        severities,
        [
            Severity::Error,   // invoice number
            Severity::Error,   // negative quantity
            Severity::Error,   // future date
            Severity::Error,   // same seller and buyer
            Severity::Warning, // 4901 at 5% instead of 0%
        ]
    );
    assert_eq!(report.errors().count(), 4);
    assert_eq!(report.warnings().count(), 1);
}

#[test]
fn test_messages_number_lines_from_one() {
    let issue = ComplianceIssue::ZeroValueLine { line: 0 };
    assert_eq!(issue.to_string(), "line 1: taxable value rounds to zero");
}

#[test]
fn test_rate_is_not_compared_against_a_fallback_heading() {
    // 84713010 falls back to heading 847130 (18%), whose rate may not apply to every tariff item
    let fallback = invoice(BUYER_SAME_STATE, vec![item("84713010", 5)]);
    assert!(check(&fallback).issues().is_empty());

    // The heading itself is compared
    let exact = invoice(BUYER_SAME_STATE, vec![item("847130", 5)]);
    assert!(matches!(
        check(&exact).issues(),
        [ComplianceIssue::RateDiffersFromHsnDefault { .. }]
    ));
}

#[test]
fn test_out_of_range_rate_is_not_also_a_rate_warning() {
    let mut invoice = invoice(BUYER_SAME_STATE, vec![item("998314", 18)]);
    invoice.line_items[0].gst_rate = BigDecimal::from(150);

    assert_eq!(
        check(&invoice).issues(),
        [ComplianceIssue::InvalidLineItem {
            line: 0,
            reason: LineItemError::RateOutOfRange(BigDecimal::from(150)),
        }]
    );
}

#[test]
fn test_rates_are_compared_against_the_master_passed_in() {
    let master: HsnMaster = serde_json::from_str(
        r#"{
            "schedule": "Custom",
            "effective_from": "2025-01-01",
            "entries": [
                { "code": "998314", "kind": "sac", "description": "IT consulting", "gst_rate": "5" }
            ]
        }"#,
    )
    .unwrap();
    let invoice = invoice(BUYER_SAME_STATE, vec![item("998314", 18), item("7010", 18)]);

    let report = validate_invoice(&invoice, date(30), &master);
    assert_eq!(
        report.issues(),
        [
            ComplianceIssue::RateDiffersFromHsnDefault {
                line: 0,
                code: "998314".to_string(),
                rate: BigDecimal::from(18),
                default: BigDecimal::from(5),
            },
            ComplianceIssue::UnknownHsnSac {
                line: 1,
                code: "7010".to_string(),
            },
        ]
    );
    assert!(check(&invoice).issues().is_empty());
}

#[test]
fn test_error_rules_report_only_errors() {
    let mut invoice = invoice(SELLER, vec![item("0000", 5), item("998314", 18)]);
    invoice.invoice_number = "INV#1".to_string();
    invoice.line_items[1].unit_price = BigDecimal::from(0);
    invoice.line_items[1].quantity = BigDecimal::from(-1);

    let errors = compliance_errors(&invoice, date(1));
    assert_eq!(errors.len(), 4);
    assert!(errors.iter().all(|e| e.severity() == Severity::Error));

    let report = validate_invoice(&invoice, date(1), HsnMaster::global());
    assert_eq!(report.errors().cloned().collect::<Vec<_>>(), errors);
    assert_eq!(report.warnings().count(), 2); // unknown 0000, zero price
}

#[test]
fn test_line_rounding_to_zero_is_a_warning() {
    let mut invoice = clean_invoice();
    invoice.line_items[1].quantity = BigDecimal::from_str("0.001").unwrap();
    invoice.line_items[1].unit_price = BigDecimal::from(1);

    let report = check(&invoice);
    assert!(report.is_compliant());
    assert_eq!(
        report.issues(),
        [ComplianceIssue::ZeroValueLine { line: 1 }]
    );

    // 0.005 rounds up to a paisa, so it is not flagged
    invoice.line_items[1].quantity = BigDecimal::from_str("0.005").unwrap();
    assert!(check(&invoice).issues().is_empty());
}

#[test]
fn test_unregistered_buyer_never_matches_the_seller() {
    let invoice = GstInvoice::new(
        "INV/2025-26/002",
        date(15),
        Gstin::parse(SELLER).unwrap(),
        Recipient::unregistered(StateCode::parse("27").unwrap()),
        vec![item("998314", 18)],
    )
    .unwrap();
    assert!(check(&invoice).issues().is_empty());
}

#[test]
fn test_exempt_line_is_not_a_rate_mismatch() {
    let exempt =
        GstLineItem::exempt("998314", "Line", BigDecimal::from(2), BigDecimal::from(500)).unwrap();
    assert!(check(&invoice(BUYER_SAME_STATE, vec![exempt]))
        .issues()
        .is_empty());

    let non_gst =
        GstLineItem::non_gst("998314", "Line", BigDecimal::from(2), BigDecimal::from(500)).unwrap();
    assert!(check(&invoice(BUYER_SAME_STATE, vec![non_gst]))
        .issues()
        .is_empty());

    // Taxable at 0% on an 18% code is still worth a warning
    let nil = check(&invoice(BUYER_SAME_STATE, vec![item("998314", 0)]));
    assert!(matches!(
        nil.issues(),
        [ComplianceIssue::RateDiffersFromHsnDefault { .. }]
    ));
}
