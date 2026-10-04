use crate::invoice::types::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

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

#[test]
fn test_invoice_tax_is_the_sum_of_lines_rounded_to_paise() {
    let line = GstLineItem::new(
        "998314",
        "Micro service",
        BigDecimal::from(1),
        BigDecimal::from_str("0.99").unwrap(),
        BigDecimal::from(5),
    )
    .unwrap();
    let invoice = invoice("27AAPFU0939F2ZU", vec![line.clone(), line]);

    // Each line: CGST = SGST = 0.02475, rounded to 0.02. Rounding once per invoice would give
    // 0.0495 -> 0.05 instead.
    for breakdown in invoice.line_breakdowns().unwrap() {
        assert_eq!(breakdown.cgst, BigDecimal::from_str("0.02").unwrap());
    }
    let total = invoice.breakdown().unwrap();
    assert_eq!(total.cgst, BigDecimal::from_str("0.04").unwrap());
    assert_eq!(total.sgst, BigDecimal::from_str("0.04").unwrap());
    assert_eq!(total.total, BigDecimal::from_str("2.06").unwrap());
}

fn unregistered_invoice(place_of_supply: &str, date: NaiveDate, price: &str) -> GstInvoice {
    let item = GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(1),
        BigDecimal::from_str(price).unwrap(),
        BigDecimal::from(0),
    )
    .unwrap();
    GstInvoice::new(
        "INV-B2C-001",
        date,
        Gstin::parse(SELLER).unwrap(),
        Recipient::unregistered(StateCode::parse(place_of_supply).unwrap()),
        vec![item],
    )
    .unwrap()
}

#[test]
fn test_state_code_accepts_known_states_only() {
    assert_eq!(StateCode::parse("29").unwrap().as_str(), "29");
    assert_eq!(StateCode::parse("97").unwrap().to_string(), "97");
    for bad in ["", "7", "007", "00", "39", "96", "99", "AB", "2 "] {
        assert!(
            matches!(StateCode::parse(bad), Err(InvoiceError::InvalidStateCode(v)) if v == bad),
            "{bad:?} should be rejected"
        );
    }
}

#[test]
fn test_unregistered_buyer_is_taxed_by_place_of_supply() {
    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    let make = |pos: &str| {
        GstInvoice::new(
            "INV-B2C-001",
            date,
            Gstin::parse(SELLER).unwrap(),
            Recipient::unregistered(StateCode::parse(pos).unwrap()),
            vec![item(18)],
        )
        .unwrap()
    };

    let inter_state = make("29").breakdown().unwrap();
    assert_eq!(inter_state.igst, BigDecimal::from(180));
    assert_eq!(inter_state.cgst, BigDecimal::from(0));

    let intra_state = make("27").breakdown().unwrap();
    assert_eq!(intra_state.cgst, BigDecimal::from(90));
    assert_eq!(intra_state.sgst, BigDecimal::from(90));
    assert_eq!(intra_state.igst, BigDecimal::from(0));
}

#[test]
fn test_recipient_exposes_gstin_and_place_of_supply() {
    let registered = Recipient::from(Gstin::parse("29AAPFU0939F1ZR").unwrap());
    assert!(registered.is_registered());
    assert_eq!(registered.place_of_supply(), "29");
    assert_eq!(registered.to_string(), "29AAPFU0939F1ZR");

    let unregistered = Recipient::unregistered(StateCode::parse("07").unwrap());
    assert!(!unregistered.is_registered());
    assert_eq!(unregistered.gstin(), None);
    assert_eq!(unregistered.place_of_supply(), "07");
    assert_eq!(
        unregistered.to_string(),
        "Unregistered (place of supply 07)"
    );
}

#[test]
fn test_supply_kind_uses_the_b2cl_threshold_for_the_invoice_date() {
    let after = NaiveDate::from_ymd_opt(2024, 8, 1).unwrap();
    let before = NaiveDate::from_ymd_opt(2024, 7, 31).unwrap();
    let kind = |pos, date, price| {
        unregistered_invoice(pos, date, price)
            .supply_kind()
            .unwrap()
    };

    assert_eq!(kind("29", after, "100000"), SupplyKind::B2cs);
    assert_eq!(kind("29", after, "100000.01"), SupplyKind::B2cl);
    assert_eq!(kind("27", after, "500000"), SupplyKind::B2cs);
    assert_eq!(kind("29", before, "250000"), SupplyKind::B2cs);
    assert_eq!(kind("29", before, "250000.01"), SupplyKind::B2cl);

    assert_eq!(
        b2cl_threshold(after),
        BigDecimal::from(B2CL_THRESHOLD_RUPEES)
    );
    assert_eq!(b2cl_threshold_revised_from(), after);
    assert_eq!(
        invoice("29AAPFU0939F1ZR", vec![item(18)])
            .supply_kind()
            .unwrap(),
        SupplyKind::B2b
    );
}

#[test]
fn test_supply_kind_counts_tax_in_the_invoice_value() {
    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    // 90,000 taxable + 18% IGST = 1,06,200, which is over the threshold
    let invoice = GstInvoice::new(
        "INV-B2C-002",
        date,
        Gstin::parse(SELLER).unwrap(),
        Recipient::unregistered(StateCode::parse("29").unwrap()),
        vec![GstLineItem::new(
            "998314",
            "IT consulting",
            BigDecimal::from(1),
            BigDecimal::from(90_000),
            BigDecimal::from(18),
        )
        .unwrap()],
    )
    .unwrap();
    assert_eq!(invoice.supply_kind().unwrap(), SupplyKind::B2cl);
}

#[test]
fn test_unregistered_invoice_round_trips_through_serde() {
    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    let invoice = unregistered_invoice("29", date, "150000");
    let json = serde_json::to_value(&invoice).unwrap();
    assert_eq!(
        json["buyer"],
        serde_json::json!({ "unregistered": { "place_of_supply": "29" } })
    );
    let back: GstInvoice = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(back, invoice);

    let mut bad = json;
    bad["buyer"]["unregistered"]["place_of_supply"] = "40".into();
    assert!(serde_json::from_value::<GstInvoice>(bad).is_err());
}

fn untaxed(treatment: SupplyTreatment) -> GstLineItem {
    let build = match treatment {
        SupplyTreatment::Exempt => GstLineItem::exempt,
        _ => GstLineItem::non_gst,
    };
    build(
        "4901",
        "Printed books",
        BigDecimal::from(2),
        BigDecimal::from(150),
    )
    .unwrap()
}

#[test]
fn test_exempt_and_non_gst_lines_carry_rate_zero() {
    for treatment in [SupplyTreatment::Exempt, SupplyTreatment::NonGst] {
        let line = untaxed(treatment);
        assert_eq!(line.treatment, treatment);
        assert_eq!(line.gst_rate, BigDecimal::from(0));
        assert!(!line.treatment.is_taxable());
        assert!(!line.charges_gst());
        assert_eq!(line.breakdown(true).unwrap().total_tax, BigDecimal::from(0));
    }

    assert_eq!(item(18).treatment, SupplyTreatment::Taxable);
    assert!(item(18).charges_gst());
    // Nil-rated: taxable, but charges nothing
    assert!(!item(0).charges_gst());
    assert!(item(0).treatment.is_taxable());
}

#[test]
fn test_untaxed_line_with_a_rate_is_rejected() {
    let json = serde_json::json!({
        "hsn_sac": "4901", "description": "Printed books", "quantity": "1",
        "unit_price": "100", "gst_rate": "18", "treatment": "exempt"
    });
    let error = serde_json::from_value::<GstLineItem>(json).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exempt lines must have a GST rate of 0, got 18"),
        "{error}"
    );

    let mut line = untaxed(SupplyTreatment::NonGst);
    line.gst_rate = BigDecimal::from(5);
    assert!(matches!(
        line.check(),
        Err(LineItemError::RateOnUntaxedSupply {
            treatment: SupplyTreatment::NonGst,
            ..
        })
    ));
}

#[test]
fn test_line_item_json_without_treatment_is_taxable() {
    let json = serde_json::json!({
        "hsn_sac": "998314", "description": "IT consulting", "quantity": "1",
        "unit_price": "100", "gst_rate": "18"
    });
    let line: GstLineItem = serde_json::from_value(json).unwrap();
    assert_eq!(line.treatment, SupplyTreatment::Taxable);

    let exempt = untaxed(SupplyTreatment::Exempt);
    let value = serde_json::to_value(&exempt).unwrap();
    assert_eq!(value["treatment"], "exempt");
    assert_eq!(
        serde_json::from_value::<GstLineItem>(value).unwrap(),
        exempt
    );
    assert_eq!(
        serde_json::to_value(SupplyTreatment::NonGst).unwrap(),
        "non_gst"
    );
    assert_eq!(SupplyTreatment::NonGst.to_string(), "non-GST");
}

#[test]
fn test_negated_breakdown_flips_every_amount() {
    let breakdown = invoice("29AAPFU0939F1ZR", vec![item(18)])
        .breakdown()
        .unwrap();
    let negated = breakdown.negated();

    assert_eq!(negated.taxable_value, BigDecimal::from(-1000));
    assert_eq!(negated.igst, BigDecimal::from(-180));
    assert_eq!(negated.total, BigDecimal::from(-1180));
    assert_eq!(negated.negated(), breakdown);
}
