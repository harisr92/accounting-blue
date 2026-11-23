//! Tests for billable lines, documents and the pricing engine

use crate::billing::discount::{Discount, DiscountError};
use crate::billing::document::*;
use crate::billing::tax::{FlatRate, TaxBreakdown, TaxComponent, TaxError};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

fn line(quantity: &str, price: &str, rate: &str) -> LineItem {
    LineItem::new("Item", dec(quantity), dec(price), dec(rate)).unwrap()
}

/// A minimal document for exercising the provided methods
struct Doc {
    lines: Vec<LineItem>,
    discount: Option<Discount>,
}

#[derive(Debug)]
enum DocError {
    Discount(DiscountError),
    Tax(TaxError),
}

impl From<DiscountError> for DocError {
    fn from(error: DiscountError) -> Self {
        Self::Discount(error)
    }
}

impl From<TaxError> for DocError {
    fn from(error: TaxError) -> Self {
        Self::Tax(error)
    }
}

impl TaxDocument for Doc {
    type Line = LineItem;
    type Regime = FlatRate;
    type Error = DocError;

    fn number(&self) -> &str {
        "DOC-1"
    }

    fn date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
    }

    fn lines(&self) -> &[LineItem] {
        &self.lines
    }

    fn document_discount(&self) -> Option<&Discount> {
        self.discount.as_ref()
    }

    fn regime(&self) -> FlatRate {
        FlatRate::new("VAT")
    }
}

#[test]
fn test_line_values_come_from_quantity_price_and_discount() {
    let item = line("3", "33.335", "20")
        .with_discount(Discount::Amount(dec("0.005")))
        .unwrap();
    assert_eq!(item.gross_value(), dec("100.005"));
    assert_eq!(item.discount_amount(), dec("0.01"));
    assert_eq!(item.taxable_value(), dec("99.995"));
    assert_eq!(line_values(&[item]), vec![dec("100.00")]);
}

#[test]
fn test_line_rejects_each_broken_field() {
    let new = |d: &str, q: &str, p: &str, r: &str| LineItem::new(d, dec(q), dec(p), dec(r));
    assert_eq!(new(" ", "1", "1", "0"), Err(LineError::EmptyDescription));
    assert_eq!(new("x", "0", "1", "0"), Err(LineError::NonPositiveQuantity));
    assert_eq!(new("x", "1", "-1", "0"), Err(LineError::NegativeUnitPrice));
    assert_eq!(
        new("x", "1", "1", "101"),
        Err(LineError::RateOutOfRange(dec("101")))
    );
    assert!(matches!(
        line("1", "10", "0").with_discount(Discount::Amount(dec("11"))),
        Err(LineError::InvalidDiscount(
            DiscountError::ExceedsValue { .. }
        ))
    ));
}

#[test]
fn test_blank_unit_leaves_the_line_without_one() {
    assert_eq!(line("1", "1", "0").with_unit("  ").unit, None);
    assert_eq!(
        line("1", "1", "0").with_unit(" kg ").unit.as_deref(),
        Some("kg")
    );
}

#[test]
fn test_line_deserialises_through_validation() {
    let ok: LineItem = serde_json::from_str(
        r#"{"description":"Pens","quantity":"2","unit_price":"5","tax_rate":"20","unit":"box"}"#,
    )
    .unwrap();
    assert_eq!(ok.unit.as_deref(), Some("box"));
    let bad = serde_json::from_str::<LineItem>(
        r#"{"description":"Pens","quantity":"0","unit_price":"5","tax_rate":"20"}"#,
    );
    assert!(bad.is_err());
}

#[test]
fn test_document_discount_is_shared_by_value_before_tax() {
    let doc = Doc {
        lines: vec![line("1", "300", "20"), line("1", "100", "0")],
        discount: Some(Discount::Amount(dec("40"))),
    };
    assert_eq!(doc.subtotal(), dec("400.00"));
    assert_eq!(doc.discount_amount(), dec("40.00"));

    let lines = doc.line_tax_breakdowns().unwrap();
    assert_eq!(lines[0].taxable_value, dec("270.00"));
    assert_eq!(lines[0].tax("VAT"), dec("54.00"));
    assert_eq!(lines[1].taxable_value, dec("90.00"));

    let total = doc.tax_breakdown().unwrap();
    assert_eq!(
        total,
        TaxBreakdown::new(dec("360.00"), vec![TaxComponent::new("VAT", dec("54.00"))])
    );
}

#[test]
fn test_document_discount_that_no_longer_fits_fails_pricing() {
    let doc = Doc {
        lines: vec![line("1", "10", "20")],
        discount: Some(Discount::Amount(dec("11"))),
    };
    assert!(doc.check_discount().is_err());
    assert!(matches!(
        doc.tax_breakdown(),
        Err(DocError::Discount(DiscountError::ExceedsValue { .. }))
    ));
}

#[test]
fn test_price_lines_reports_subtotal_discount_and_breakdowns_together() {
    let doc = Doc {
        lines: vec![line("2", "50", "10")],
        discount: Some(Discount::Percent(dec("10"))),
    };
    let priced = price_lines(&doc).unwrap();
    assert_eq!(priced.subtotal, dec("100.00"));
    assert_eq!(priced.discount, dec("10.00"));
    assert_eq!(priced.breakdowns[0].total, dec("99.00"));
}

#[test]
fn test_regime_errors_reach_the_document_error() {
    let doc = Doc {
        lines: vec![LineItem {
            tax_rate: dec("150"),
            ..line("1", "10", "0")
        }],
        discount: None,
    };
    assert!(matches!(
        doc.tax_breakdown(),
        Err(DocError::Tax(TaxError::RateOutOfRange(_)))
    ));
}
