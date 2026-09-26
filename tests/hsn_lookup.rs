//! Tests for the HSN/SAC master data and default-rate line items

use accounting_core::invoice::{
    GstInvoice, GstLineItem, Gstin, HsnMaster, HsnSacKind, InvoiceError,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashSet;

/// Whether a code has the shape of an HSN/SAC code: 4, 6 or 8 ASCII digits
fn is_well_formed(code: &str) -> bool {
    matches!(code.len(), 4 | 6 | 8) && code.bytes().all(|b| b.is_ascii_digit())
}

fn line(hsn: &str) -> Result<GstLineItem, InvoiceError> {
    GstLineItem::with_default_rate(
        hsn.to_string(),
        "Item".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(100),
    )
}

#[test]
fn test_master_integrity() {
    let master = HsnMaster::global();
    assert_eq!(master.schedule(), "GST 2.0");
    assert_eq!(
        master.effective_from(),
        NaiveDate::from_ymd_opt(2025, 9, 22).unwrap()
    );
    assert!(master.entries().len() >= 50);

    let allowed_rates: Vec<BigDecimal> = ["0", "0.25", "3", "5", "18", "40"]
        .iter()
        .map(|r| r.parse().unwrap())
        .collect();
    let mut seen = HashSet::new();

    for entry in master.entries() {
        assert!(is_well_formed(&entry.code), "{} is malformed", entry.code);
        assert!(seen.insert(&entry.code), "{} is duplicated", entry.code);
        assert!(!entry.description.trim().is_empty(), "{}", entry.code);
        assert!(
            allowed_rates.contains(&entry.gst_rate),
            "{} has rate {} outside the GST 2.0 slabs",
            entry.code,
            entry.gst_rate
        );
    }

    let kinds: HashSet<_> = master.entries().iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&HsnSacKind::Hsn));
    assert!(kinds.contains(&HsnSacKind::Sac));
}

#[test]
fn test_lookup_exact() {
    let master = HsnMaster::global();

    let wheat = master.lookup("1001").unwrap();
    assert_eq!(wheat.kind, HsnSacKind::Hsn);
    assert_eq!(wheat.gst_rate, BigDecimal::from(0));

    assert_eq!(master.default_rate("7010"), Some(BigDecimal::from(18)));
    assert_eq!(master.default_rate("7108"), Some(BigDecimal::from(3)));
    assert_eq!(master.lookup("998314").unwrap().kind, HsnSacKind::Sac);
}

#[test]
fn test_lookup_falls_back_to_heading() {
    let master = HsnMaster::global();

    // 8-digit tariff item → 6-digit subheading
    assert_eq!(master.lookup("84713010").unwrap().code, "847130");
    // 6-digit subheading → 4-digit heading
    assert_eq!(master.lookup("847150").unwrap().code, "8471");
    // 8-digit → 4-digit when there is no 6-digit entry
    assert_eq!(master.lookup("70109000").unwrap().code, "7010");
    // an exact 8-digit entry wins over its heading
    assert_eq!(master.lookup("48202000").unwrap().code, "48202000");
}

#[test]
fn test_lookup_misses() {
    let master = HsnMaster::global();

    assert!(master.lookup("0000").is_none());
    assert!(master.lookup("48201000").is_none()); // only 48202000 is known
    assert!(master.lookup("847").is_none());
    assert!(master.lookup("84A1").is_none());
    assert!(master.lookup("").is_none());
    assert!(master.default_rate("12345").is_none());
}

#[test]
fn test_line_item_with_default_rate() {
    assert_eq!(line("1001").unwrap().gst_rate, BigDecimal::from(0));
    assert_eq!(line("7010").unwrap().gst_rate, BigDecimal::from(18));
    assert_eq!(line("84713010").unwrap().gst_rate, BigDecimal::from(18));
    assert!(matches!(line("0000"), Err(InvoiceError::UnknownHsnSac(code)) if code == "0000"));
    assert!(matches!(line("84A1"), Err(InvoiceError::InvalidHsnSac(_))));
    assert!(matches!(line("847"), Err(InvoiceError::InvalidHsnSac(_))));
}

#[test]
fn test_invoice_with_default_rates() {
    let lines = vec![
        GstLineItem::with_default_rate(
            "7010".to_string(),
            "Glass jars".to_string(),
            BigDecimal::from(10),
            BigDecimal::from(100),
        )
        .unwrap(),
        GstLineItem::with_default_rate(
            "1001".to_string(),
            "Wheat".to_string(),
            BigDecimal::from(5),
            BigDecimal::from(200),
        )
        .unwrap(),
    ];
    // Maharashtra (27) → Karnataka (29): inter-state, so IGST
    let invoice = GstInvoice::new(
        "INV/2024-25/001".to_string(),
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse("27AAPFU0939F1ZV").unwrap(),
        Gstin::parse("29AAPFU0939F1ZR").unwrap(),
        lines,
    )
    .unwrap();
    let breakdown = invoice.breakdown().unwrap();

    assert_eq!(breakdown.taxable_value, BigDecimal::from(2000));
    assert_eq!(breakdown.igst, BigDecimal::from(180)); // 18% of 1000 + 0% of 1000
    assert_eq!(breakdown.total, BigDecimal::from(2180));
}
