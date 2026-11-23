use crate::invoice::payment::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn dec(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).unwrap()
}

#[test]
fn test_upi_link_percent_encodes_every_value() {
    let uri = upi_uri("shop@ok hdfc", "Ravi & Sons", &dec("10"), "INV/2025-26/001");
    assert_eq!(
        uri,
        "upi://pay?pa=shop%40ok%20hdfc&pn=Ravi%20%26%20Sons&am=10.00&cu=INR&tn=INV%2F2025-26%2F001"
    );
}

#[test]
fn test_upi_link_encodes_non_ascii_names_as_utf8() {
    let uri = upi_uri("a@b", "ಕನ್ನಡ", &dec("1"), "x");
    assert!(uri.contains("pn=%E0%B2%95%E0%B2%A8"), "{uri}");
    assert!(uri.is_ascii());
}

#[test]
fn test_upi_link_amount_has_two_decimals() {
    let amount = |value: &str| {
        let uri = upi_uri("a@b", "A", &dec(value), "N");
        uri.split("&am=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(amount("1180"), "1180.00");
    assert_eq!(amount("0.5"), "0.50");
    assert_eq!(amount("99.995"), "100.00");
}

#[test]
fn test_payment_details_with_only_blank_fields_are_empty() {
    assert!(PaymentDetails::default().is_empty());
    let blank = PaymentDetails {
        payee_name: Some("Acme".into()),
        bank_name: Some("  ".into()),
        amount_due: Some(dec("5")),
        ..PaymentDetails::default()
    };
    assert!(blank.is_empty());
    let trimmed = blank.trimmed();
    assert_eq!(trimmed.bank_name, None);
    assert_eq!(trimmed.payee_name.as_deref(), Some("Acme"));
}

#[test]
fn test_one_bank_field_makes_payment_details_non_empty() {
    let details = PaymentDetails {
        ifsc: Some(" HDFC0001234 ".into()),
        ..PaymentDetails::default()
    };
    assert!(!details.is_empty());
    assert_eq!(details.trimmed().ifsc.as_deref(), Some("HDFC0001234"));
}
