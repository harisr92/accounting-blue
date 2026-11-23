//! Tests for the money helpers

use crate::money::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

#[test]
fn test_rounding_takes_halves_away_from_zero() {
    assert_eq!(round_money(&dec("0.005")), dec("0.01"));
    assert_eq!(round_money(&dec("-0.005")), dec("-0.01"));
    assert_eq!(round_money(&dec("0.0049")), dec("0.00"));
}

#[test]
fn test_rounded_amounts_have_two_decimal_places() {
    assert_eq!(round_money(&BigDecimal::from(7)).to_string(), "7.00");
    assert_eq!(MONEY_SCALE, 2);
}

#[test]
fn test_percent_of_is_exact() {
    assert_eq!(percent_of(&dec("33.33"), &dec("5")), dec("1.6665"));
    assert_eq!(
        percent_of(&BigDecimal::from(0), &dec("18")),
        BigDecimal::from(0)
    );
}
