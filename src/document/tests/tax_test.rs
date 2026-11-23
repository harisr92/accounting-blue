//! Tests for tax regimes and breakdowns

use crate::document::tax::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

fn vat(amount: &str) -> TaxComponent {
    TaxComponent::new("VAT", dec(amount))
}

#[test]
fn test_breakdown_totals_add_the_components_to_the_taxable_value() {
    let breakdown = TaxBreakdown::new(
        dec("100"),
        vec![
            TaxComponent::new("CGST", dec("9")),
            TaxComponent::new("SGST", dec("9")),
        ],
    );
    assert_eq!(breakdown.total_tax, dec("18"));
    assert_eq!(breakdown.total, dec("118"));
}

#[test]
fn test_summing_breakdowns_merges_components_by_code_in_first_seen_order() {
    let lines = [
        TaxBreakdown::new(dec("100"), vec![vat("20")]),
        TaxBreakdown::new(
            dec("50"),
            vec![TaxComponent::new("LEVY", dec("1")), vat("10")],
        ),
    ];
    let total: TaxBreakdown = lines.iter().sum();
    assert_eq!(total.taxable_value, dec("150"));
    assert_eq!(
        total.components,
        vec![vat("30"), TaxComponent::new("LEVY", dec("1"))]
    );
    assert_eq!(total.total, dec("181"));
}

#[test]
fn test_summing_no_breakdowns_gives_zero() {
    let total: TaxBreakdown = std::iter::empty::<&TaxBreakdown>().sum();
    assert_eq!(total, TaxBreakdown::default());
}

#[test]
fn test_negated_breakdown_flips_every_amount() {
    let breakdown = TaxBreakdown::new(dec("100"), vec![vat("20")]).negated();
    assert_eq!(breakdown.taxable_value, dec("-100"));
    assert_eq!(breakdown.tax("VAT"), dec("-20"));
    assert_eq!(breakdown.total, dec("-120"));
}

#[test]
fn test_no_tax_rounds_the_value_and_charges_nothing() {
    let breakdown = NoTax.line_tax(&dec("10.005"), &dec("18")).unwrap();
    assert_eq!(breakdown.taxable_value, dec("10.01"));
    assert!(breakdown.components.is_empty());
    assert_eq!(breakdown.total, dec("10.01"));
}

#[test]
fn test_flat_rate_taxes_the_rounded_value_and_rounds_the_tax() {
    let breakdown = FlatRate::new("VAT")
        .line_tax(&dec("33.333"), &dec("5"))
        .unwrap();
    assert_eq!(breakdown.taxable_value, dec("33.33"));
    assert_eq!(breakdown.tax("VAT"), dec("1.67"));
    assert_eq!(breakdown.total, dec("35.00"));
}

#[test]
fn test_flat_rate_at_zero_keeps_a_zero_component() {
    let breakdown = FlatRate::new("VAT")
        .line_tax(&dec("100"), &dec("0"))
        .unwrap();
    assert_eq!(breakdown.components, vec![vat("0.00")]);
}

#[test]
fn test_flat_rate_rejects_rates_outside_zero_to_a_hundred() {
    let regime = FlatRate::new("VAT");
    assert!(matches!(
        regime.line_tax(&dec("100"), &dec("-1")),
        Err(TaxError::RateOutOfRange(_))
    ));
    assert!(matches!(
        regime.line_tax(&dec("100"), &dec("100.01")),
        Err(TaxError::RateOutOfRange(_))
    ));
    assert!(regime.line_tax(&dec("100"), &dec("100")).is_ok());
}
