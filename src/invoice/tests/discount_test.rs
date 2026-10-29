use crate::invoice::discount::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).unwrap()
}

fn strings(values: &[BigDecimal]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn test_percentage_discount_is_rounded_to_paise() {
    let discount = Discount::Percent(dec("10"));
    assert_eq!(discount.amount_off(&dec("1999.99")).to_string(), "200.00");
    assert_eq!(discount.amount_off(&dec("1999.94")).to_string(), "199.99");
}

#[test]
fn test_amount_discount_is_taken_as_given() {
    let discount = Discount::Amount(dec("99.5"));
    assert_eq!(discount.amount_off(&dec("1999")).to_string(), "99.50");
}

#[test]
fn test_rounded_discount_never_exceeds_its_base() {
    let base = dec("3.335");
    assert_eq!(Discount::Percent(dec("100")).amount_off(&base), base);
    assert_eq!(Discount::Amount(dec("3.335")).amount_off(&base), base);
}

#[test]
fn test_discount_within_range_is_accepted() {
    let base = dec("1999");
    assert!(Discount::Percent(dec("0")).check(&base).is_ok());
    assert!(Discount::Percent(dec("100")).check(&base).is_ok());
    assert!(Discount::Amount(dec("0")).check(&base).is_ok());
    assert!(Discount::Amount(dec("1999")).check(&base).is_ok());
}

#[test]
fn test_discount_out_of_range_is_rejected() {
    let base = dec("1999");
    assert_eq!(
        Discount::Percent(dec("100.01")).check(&base),
        Err(DiscountError::PercentOutOfRange(dec("100.01")))
    );
    assert_eq!(
        Discount::Percent(dec("-1")).check(&base),
        Err(DiscountError::PercentOutOfRange(dec("-1")))
    );
    assert_eq!(
        Discount::Amount(dec("-1")).check(&base),
        Err(DiscountError::NegativeAmount(dec("-1")))
    );
    assert_eq!(
        Discount::Amount(dec("1999.01")).check(&base),
        Err(DiscountError::ExceedsValue {
            discount: dec("1999.01"),
            value: base,
        })
    );
}

#[test]
fn test_discount_displays_its_rate_or_amount() {
    assert_eq!(Discount::Percent(dec("12.50")).to_string(), "12.5%");
    assert_eq!(Discount::Amount(dec("500.00")).to_string(), "500");
}

#[test]
fn test_discount_serialises_by_kind() {
    let discount: Discount = serde_json::from_str(r#"{"percent": "10"}"#).unwrap();
    assert_eq!(discount, Discount::Percent(dec("10")));

    let discount = Discount::Amount(dec("500"));
    let json = serde_json::to_string(&discount).unwrap();
    assert_eq!(serde_json::from_str::<Discount>(&json).unwrap(), discount);
    assert!(json.contains("amount"));
}

#[test]
fn test_apportioned_shares_add_up_to_the_total() {
    let bases = [dec("1"), dec("1"), dec("1")];
    assert_eq!(
        strings(&apportion(&dec("100"), &bases)),
        ["33.33", "33.33", "33.34"]
    );
}

#[test]
fn test_rounding_remainder_goes_to_the_largest_base() {
    // 1.666.. rounds up on every line, so the largest takes back the extra paisa
    let bases = [dec("1"), dec("1"), dec("4")];
    let shares = apportion(&dec("10"), &bases);
    assert_eq!(strings(&shares), ["1.67", "1.67", "6.66"]);
    assert_eq!(shares.iter().sum::<BigDecimal>(), dec("10"));
}

#[test]
fn test_shares_follow_the_bases() {
    let bases = [dec("1000"), dec("500")];
    assert_eq!(strings(&apportion(&dec("100"), &bases)), ["66.67", "33.33"]);
}

#[test]
fn test_zero_bases_get_zero_shares() {
    let bases = [dec("0"), dec("0")];
    assert_eq!(
        apportion(&dec("10"), &bases),
        [BigDecimal::from(0), BigDecimal::from(0)]
    );
    assert!(apportion(&dec("10"), &[]).is_empty());
}

#[test]
fn test_discount_errors_convert_into_the_crate_error() {
    fn check(discount: &Discount) -> crate::Result<()> {
        discount.check(&dec("100"))?;
        Ok(())
    }
    assert!(matches!(
        check(&Discount::Amount(dec("-1"))),
        Err(crate::Error::Discount(DiscountError::NegativeAmount(_)))
    ));
}
