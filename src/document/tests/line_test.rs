//! Tests for document lines

use crate::document::discount::{Discount, DiscountError};
use crate::document::line::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

fn line(quantity: &str, price: &str, rate: &str) -> LineItem {
    LineItem::new("Item", dec(quantity), dec(price), dec(rate)).unwrap()
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
