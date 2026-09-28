use crate::tax::gst::*;
use bigdecimal::BigDecimal;

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
