use crate::tax::gst::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

/// Every amount in the calculation, in field order
fn amounts(c: &GstCalculation) -> [&BigDecimal; 6] {
    [
        &c.base_amount,
        &c.cgst_amount,
        &c.sgst_amount,
        &c.igst_amount,
        &c.total_gst_amount,
        &c.total_amount,
    ]
}

fn assert_in_paise(c: &GstCalculation) {
    for amount in amounts(c) {
        assert_eq!(amount, &round_to_paise(amount), "{amount} is not in paise");
    }
}

#[test]
fn test_gst_rate_intra_state() {
    let rate = GstRate::intra_state(BigDecimal::from(18));
    assert_eq!(rate.total_rate, BigDecimal::from(18));
    assert_eq!(rate.cgst_rate, BigDecimal::from(9));
    assert_eq!(rate.sgst_rate, BigDecimal::from(9));
    assert_eq!(rate.igst_rate, BigDecimal::from(0));
    assert!(rate.validate().is_ok());
}

#[test]
fn test_gst_rate_inter_state() {
    let rate = GstRate::inter_state(BigDecimal::from(18));
    assert_eq!(rate.total_rate, BigDecimal::from(18));
    assert_eq!(rate.cgst_rate, BigDecimal::from(0));
    assert_eq!(rate.sgst_rate, BigDecimal::from(0));
    assert_eq!(rate.igst_rate, BigDecimal::from(18));
    assert!(rate.validate().is_ok());
}

#[test]
fn test_gst_calculation() {
    let base_amount = BigDecimal::from(1000);
    let gst_rate = GstRate::intra_state(BigDecimal::from(18));

    let calculation = GstCalculation::calculate(base_amount, gst_rate).unwrap();

    assert_eq!(calculation.base_amount, BigDecimal::from(1000));
    assert_eq!(calculation.cgst_amount, BigDecimal::from(90));
    assert_eq!(calculation.sgst_amount, BigDecimal::from(90));
    assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
    assert_eq!(calculation.total_amount, BigDecimal::from(1180));
}

#[test]
fn test_gst_reverse_calculation() {
    let total_amount = BigDecimal::from(1180);
    let gst_rate = GstRate::intra_state(BigDecimal::from(18));

    let calculation = GstCalculation::reverse_calculate(total_amount, gst_rate).unwrap();

    assert_eq!(calculation.total_amount, BigDecimal::from(1180));
    assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
    assert_eq!(calculation.base_amount, BigDecimal::from(1000));
}

#[test]
fn test_gst_calculator() {
    let calculator = GstCalculator::new(false); // intra-state default

    let calculation = calculator
        .calculate_by_category(BigDecimal::from(1000), GstCategory::Higher, None)
        .unwrap();

    assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
    assert_eq!(calculation.cgst_amount, BigDecimal::from(90));
    assert_eq!(calculation.sgst_amount, BigDecimal::from(90));
}
#[test]
fn test_rate_validation_errors_are_typed() {
    let mismatched = GstRate {
        total_rate: BigDecimal::from(18),
        cgst_rate: BigDecimal::from(9),
        sgst_rate: BigDecimal::from(8),
        igst_rate: BigDecimal::from(0),
    };
    assert!(matches!(
        mismatched.validate(),
        Err(GstError::ComponentsMismatch { .. })
    ));

    let unequal = GstRate {
        total_rate: BigDecimal::from(18),
        cgst_rate: BigDecimal::from(10),
        sgst_rate: BigDecimal::from(8),
        igst_rate: BigDecimal::from(0),
    };
    assert!(matches!(
        unequal.validate(),
        Err(GstError::UnequalSplit { .. })
    ));

    let mixed = GstRate {
        total_rate: BigDecimal::from(18),
        cgst_rate: BigDecimal::from(4),
        sgst_rate: BigDecimal::from(4),
        igst_rate: BigDecimal::from(10),
    };
    assert_eq!(mixed.validate(), Err(GstError::MixedIgstAndSplit));
}

#[test]
fn test_for_supply_picks_the_split() {
    assert_eq!(
        GstRate::for_supply(BigDecimal::from(12), true),
        GstRate::inter_state(BigDecimal::from(12))
    );
    assert_eq!(
        GstCategory::Standard.rate_for_supply(false),
        GstCategory::Standard.intra_state_rate()
    );
}

#[test]
fn test_round_to_paise_rounds_halves_away_from_zero() {
    assert_eq!(round_to_paise(&dec("0.02475")), dec("0.02"));
    assert_eq!(round_to_paise(&dec("0.025")), dec("0.03"));
    assert_eq!(round_to_paise(&dec("1.005")), dec("1.01"));
    assert_eq!(round_to_paise(&dec("-0.025")), dec("-0.03"));
    assert_eq!(round_to_paise(&dec("0.0249999")), dec("0.02"));
    assert_eq!(round_to_paise(&dec("90")).to_string(), "90.00");
}

#[test]
fn test_intra_state_components_are_rounded_separately() {
    // 5% of 0.99 is 0.0495, split as 0.02475 + 0.02475
    let calc =
        GstCalculation::calculate(dec("0.99"), GstRate::intra_state(BigDecimal::from(5))).unwrap();

    assert_eq!(calc.cgst_amount, dec("0.02"));
    assert_eq!(calc.sgst_amount, dec("0.02"));
    assert_eq!(calc.total_gst_amount, dec("0.04"));
    assert_eq!(calc.total_amount, dec("1.03"));
    assert_in_paise(&calc);
    assert!(calc.gst_rate.validate().is_ok());
}

#[test]
fn test_inter_state_igst_is_rounded() {
    let calc =
        GstCalculation::calculate(dec("0.99"), GstRate::inter_state(BigDecimal::from(5))).unwrap();

    assert_eq!(calc.igst_amount, dec("0.05"));
    assert_eq!(calc.total_amount, dec("1.04"));
    assert_in_paise(&calc);
}

#[test]
fn test_base_amount_is_rounded_before_tax() {
    // 2.5 units at 33.33 is 83.325
    let calc = GstCalculation::calculate(dec("83.325"), GstRate::inter_state(BigDecimal::from(18)))
        .unwrap();

    assert_eq!(calc.base_amount, dec("83.33"));
    assert_eq!(calc.igst_amount, dec("15.00"));
    assert_eq!(calc.total_amount, dec("98.33"));
}

#[test]
fn test_reverse_calculation_with_non_terminating_division_is_in_paise() {
    // 1000 / 1.18 = 847.457627...
    let calc =
        GstCalculation::reverse_calculate(dec("1000"), GstRate::intra_state(BigDecimal::from(18)))
            .unwrap();

    assert_eq!(calc.base_amount, dec("847.46"));
    assert_eq!(calc.cgst_amount, dec("76.27"));
    assert_eq!(calc.sgst_amount, dec("76.27"));
    assert_eq!(calc.total_amount, dec("1000.00"));
    assert_in_paise(&calc);
}

#[test]
fn test_reverse_calculation_drifts_at_most_a_paisa_at_gst_slabs() {
    // 1.00 at 18%: base 0.85, CGST = SGST = 0.08, total 1.01
    let calc =
        GstCalculation::reverse_calculate(dec("1"), GstRate::intra_state(BigDecimal::from(18)))
            .unwrap();
    assert_eq!(calc.total_amount, dec("1.01"));

    let paisa = dec("0.01");
    for rate in ["0.25", "3", "5", "18", "40"] {
        for is_inter_state in [false, true] {
            for paise in (1..=20_000).step_by(7) {
                let total = BigDecimal::from(paise) / BigDecimal::from(100);
                let rate = GstRate::for_supply(dec(rate), is_inter_state);
                let calc = GstCalculation::reverse_calculate(total.clone(), rate).unwrap();
                assert_in_paise(&calc);
                assert!((&calc.total_amount - &total).abs() <= paisa, "{total}");
            }
        }
    }
}
