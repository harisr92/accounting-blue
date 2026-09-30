use crate::utils::formatting::*;
use bigdecimal::BigDecimal;

fn amount(value: &str) -> BigDecimal {
    value.parse().unwrap()
}

#[test]
fn test_amounts_are_grouped_the_indian_way() {
    let cases = [
        ("0", "0.00"),
        ("999", "999.00"),
        ("1000", "1,000.00"),
        ("99999.99", "99,999.99"),
        ("100000", "1,00,000.00"),
        ("12345678.9", "1,23,45,678.90"),
        ("123456789012", "1,23,45,67,89,012.00"),
        ("-1234.5", "-1,234.50"),
    ];
    for (value, expected) in cases {
        assert_eq!(format_inr(&amount(value)), expected, "{value}");
    }
}

#[test]
fn test_formatting_rounds_to_the_paisa() {
    assert_eq!(format_inr(&amount("0.005")), "0.01");
    assert_eq!(format_inr(&amount("999.995")), "1,000.00");
    assert_eq!(format_inr(&amount("-0.001")), "0.00");
}

#[test]
fn test_amounts_are_spelled_out_in_lakh_and_crore() {
    let cases = [
        ("0", "Rupees Zero Only"),
        ("0.5", "Fifty Paise Only"),
        ("1", "Rupees One Only"),
        ("15", "Rupees Fifteen Only"),
        ("40", "Rupees Forty Only"),
        ("101", "Rupees One Hundred One Only"),
        ("17700", "Rupees Seventeen Thousand Seven Hundred Only"),
        ("100000", "Rupees One Lakh Only"),
        ("10000000", "Rupees One Crore Only"),
        (
            "123456789.89",
            "Rupees Twelve Crore Thirty Four Lakh Fifty Six Thousand Seven Hundred Eighty Nine \
             and Eighty Nine Paise Only",
        ),
        ("10000000000", "Rupees One Thousand Crore Only"),
        ("-21.05", "Minus Rupees Twenty One and Five Paise Only"),
    ];
    for (value, expected) in cases {
        assert_eq!(amount_in_words(&amount(value)), expected, "{value}");
    }
}
