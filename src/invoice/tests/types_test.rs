use crate::invoice::types::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

const SELLER: &str = "27AAPFU0939F1ZV";

fn gstin_with_checksum(body: &str) -> String {
    format!("{body}{}", gstin_checksum(body.as_bytes()).unwrap() as char)
}

fn item(rate: i32) -> GstLineItem {
    GstLineItem::new(
        "998314".to_string(),
        "IT consulting".to_string(),
        BigDecimal::from(2),
        BigDecimal::from(500),
        BigDecimal::from(rate),
    )
    .unwrap()
}

fn invoice(buyer: &str, line_items: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        "INV/2024-25/001".to_string(),
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(buyer).unwrap(),
        line_items,
    )
    .unwrap()
}

#[test]
fn test_gstin_valid() {
    let gstin = Gstin::parse(SELLER).unwrap();
    assert_eq!(gstin.state_code(), "27");
    assert_eq!(gstin.pan(), "AAPFU0939F");
    assert_eq!(gstin.to_string(), SELLER);
}

#[test]
fn test_gstin_normalises_case() {
    let gstin = Gstin::parse(&SELLER.to_lowercase()).unwrap();
    assert_eq!(gstin.as_str(), SELLER);
}

#[test]
fn test_gstin_rejects_bad_checksum() {
    assert!(matches!(
        Gstin::parse("27AAPFU0939F1ZA"),
        Err(InvoiceError::InvalidGstin {
            reason: GstinError::Checksum { expected: 'V' },
            ..
        })
    ));
}

#[test]
fn test_gstin_rejects_malformed() {
    let cases = [
        "",
        "27AAPFU0939F1Z",                       // too short
        "27AAPFU0939F1ZVX",                     // too long
        &gstin_with_checksum("00AAPFU0939F1Z"), // state code 00
        &gstin_with_checksum("40AAPFU0939F1Z"), // unassigned state code
        &gstin_with_checksum("27AAPF10939F1Z"), // digit in PAN letters
        &gstin_with_checksum("27AAPFU09X9F1Z"), // letter in PAN digits
        &gstin_with_checksum("27AAPFU093991Z"), // PAN ends in a digit
        &gstin_with_checksum("27AAPFU0939F0Z"), // entity number 0
        &gstin_with_checksum("27AAPFU0939F1Y"), // 14th character not Z
        "27AAPFU0939F1Z-",                      // non-alphanumeric
        "+7AAPFU0939F1ZV",                      // sign in the state code
        "२७AAPFU0939F1ZV",                      // non-ASCII digits
    ];

    for case in cases {
        assert!(Gstin::parse(case).is_err(), "{case} should be rejected");
    }
}

#[test]
fn test_gstin_accepts_special_state_codes() {
    for body in [
        "01AAPFU0939F1Z",
        "38AAPFU0939F1Z",
        "97AAPFU0939F1Z",
        "99AAPFU0939F1Z",
    ] {
        assert!(Gstin::parse(&gstin_with_checksum(body)).is_ok(), "{body}");
    }
}

#[test]
fn test_gstin_serde_validates() {
    let json = serde_json::to_string(&Gstin::parse(SELLER).unwrap()).unwrap();
    assert_eq!(json, format!("\"{SELLER}\""));
    assert!(serde_json::from_str::<Gstin>(&json).is_ok());
    assert!(serde_json::from_str::<Gstin>("\"27AAPFU0939F1ZA\"").is_err());
}

#[test]
fn test_invoice_serde_validates() {
    let buyer = gstin_with_checksum("27AABCT1332L1Z");
    let invoice = invoice(&buyer, vec![item(18)]);
    let json = serde_json::to_string(&invoice).unwrap();
    assert_eq!(serde_json::from_str::<GstInvoice>(&json).unwrap(), invoice);

    let bad_number = json.replace("INV/2024-25/001", "");
    assert!(serde_json::from_str::<GstInvoice>(&bad_number).is_err());

    let line = serde_json::to_string(&item(18)).unwrap();
    let bad_rate = line.replace("\"gst_rate\":\"18\"", "\"gst_rate\":\"150\"");
    assert_ne!(bad_rate, line);
    assert!(serde_json::from_str::<GstLineItem>(&bad_rate).is_err());
}

#[test]
fn test_line_item_validation() {
    let valid = |hsn: &str, qty: i32, price: i32, rate: i32| {
        GstLineItem::new(
            hsn.to_string(),
            "Item".to_string(),
            BigDecimal::from(qty),
            BigDecimal::from(price),
            BigDecimal::from(rate),
        )
        .is_ok()
    };

    assert!(valid("8471", 1, 100, 18));
    assert!(valid("847130", 1, 100, 18));
    assert!(valid("84713010", 1, 0, 0));
    assert!(!valid("847", 1, 100, 18));
    assert!(!valid("84713", 1, 100, 18));
    assert!(!valid("84A1", 1, 100, 18));
    assert!(!valid("8471", 0, 100, 18));
    assert!(!valid("8471", 1, -1, 18));
    assert!(!valid("8471", 1, 100, -5));
    assert!(!valid("8471", 1, 100, 101));
}

#[test]
fn test_invoice_number_validation() {
    assert!(validate_invoice_number("INV/2024-25/001").is_ok());
    assert!(validate_invoice_number("").is_err());
    assert!(validate_invoice_number("INV/2024-25/00001").is_err()); // 17 chars
    assert!(validate_invoice_number("INV 001").is_err());
    assert!(validate_invoice_number("INV#001").is_err());
}

#[test]
fn test_invoice_requires_line_items() {
    let result = GstInvoice::new(
        "INV-1".to_string(),
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        vec![],
    );
    assert!(matches!(result, Err(InvoiceError::EmptyInvoice)));
}

#[test]
fn test_intra_state_breakdown() {
    let buyer = gstin_with_checksum("27AABCT1332L1Z");
    let invoice = invoice(&buyer, vec![item(18), item(5)]);
    assert!(!invoice.is_inter_state());

    let breakdown = invoice.breakdown().unwrap();
    assert_eq!(breakdown.taxable_value, BigDecimal::from(2000));
    assert_eq!(breakdown.cgst, BigDecimal::from(115)); // 90 + 25
    assert_eq!(breakdown.sgst, BigDecimal::from(115));
    assert_eq!(breakdown.igst, BigDecimal::from(0));
    assert_eq!(breakdown.total_tax, BigDecimal::from(230));
    assert_eq!(breakdown.total, BigDecimal::from(2230));
}

#[test]
fn test_inter_state_breakdown() {
    let buyer = gstin_with_checksum("29AABCT1332L1Z");
    let invoice = invoice(&buyer, vec![item(18)]);
    assert!(invoice.is_inter_state());

    let lines = invoice.line_breakdowns().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].igst, BigDecimal::from(180));

    let breakdown = invoice.breakdown().unwrap();
    assert_eq!(breakdown.cgst, BigDecimal::from(0));
    assert_eq!(breakdown.sgst, BigDecimal::from(0));
    assert_eq!(breakdown.igst, BigDecimal::from(180));
    assert_eq!(breakdown.total, BigDecimal::from(1180));
}
