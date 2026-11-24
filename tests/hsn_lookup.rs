//! Tests for the HSN/SAC master data and default-rate line items
#![cfg(feature = "india")]

use accounting_core::gst::invoice::{
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
    assert_eq!(master.lists_every_code(), cfg!(feature = "hsn-catalog"));
    // The portal's whole list, not only the codes with a checked rate.
    #[cfg(feature = "hsn-catalog")]
    assert!(master.entries().len() > 20_000);

    let allowed_rates: Vec<BigDecimal> = ["0", "0.25", "3", "5", "18", "40"]
        .iter()
        .map(|r| r.parse().unwrap())
        .collect();
    let mut seen = HashSet::new();

    for entry in master.entries() {
        assert!(is_well_formed(&entry.code), "{} is malformed", entry.code);
        assert!(seen.insert(&entry.code), "{} is duplicated", entry.code);
        assert!(!entry.description.trim().is_empty(), "{}", entry.code);
        if let Some(rate) = &entry.gst_rate {
            assert!(
                allowed_rates.contains(rate),
                "{} has rate {} outside the GST 2.0 slabs",
                entry.code,
                rate
            );
        }
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
    assert_eq!(wheat.gst_rate, Some(BigDecimal::from(0)));

    assert_eq!(master.default_rate("7010"), Some(BigDecimal::from(18)));
    assert_eq!(master.default_rate("7108"), Some(BigDecimal::from(3)));
    assert_eq!(master.lookup("998314").unwrap().kind, HsnSacKind::Sac);
}

#[test]
#[cfg(feature = "hsn-catalog")]
fn test_every_portal_code_is_known_with_its_description() {
    let master = HsnMaster::global();

    let laptop = master.lookup("84713010").unwrap();
    assert_eq!(laptop.code, "84713010");
    assert_eq!(laptop.description, "PERSONAL COMPUTER");
    assert_eq!(laptop.gst_rate, None, "the portal gives no rate");
    // A checked code keeps its checked wording and rate.
    assert_eq!(
        master.lookup("847130").unwrap().gst_rate,
        Some(BigDecimal::from(18))
    );
    // A code that lost its leading zero in the portal's file has it back.
    assert_eq!(master.lookup("030559").unwrap().code, "030559");
}

#[test]
fn test_a_code_without_a_rate_takes_its_nearest_headings() {
    let master = HsnMaster::global();

    // 84713010 → 847130 (18%), 70109000 → 7010 (18%)
    assert_eq!(master.default_rate("84713010"), Some(BigDecimal::from(18)));
    assert_eq!(master.default_rate("70109000"), Some(BigDecimal::from(18)));
    // an exact checked entry wins over its heading
    assert!(master.lookup("48202000").unwrap().gst_rate.is_some());
}

#[test]
fn test_lookup_falls_back_to_a_heading_the_list_holds() {
    // 84719999 is not a tariff item, but its heading 8471 is.
    assert_eq!(HsnMaster::global().lookup("84719999").unwrap().code, "8471");
}

#[test]
#[cfg(feature = "hsn-catalog")]
fn test_search_by_code_lists_headings_first() {
    let found: Vec<&str> = HsnMaster::global()
        .search("8471", 3)
        .iter()
        .map(|e| e.code.as_str())
        .collect();
    assert_eq!(found[0], "8471");
    assert_eq!(found.len(), 3);
    assert!(found[1..]
        .iter()
        .all(|code| code.starts_with("8471") && code.len() == 6));
}

#[test]
#[cfg(feature = "hsn-catalog")]
fn test_search_by_words_matches_every_word_ignoring_case() {
    let found = HsnMaster::global().search("personal Computer", 10);
    assert!(found.iter().any(|e| e.code == "84713010"));
    assert!(found.iter().all(|e| {
        let text = e.description.to_lowercase();
        text.contains("personal") && text.contains("computer")
    }));
    assert!(HsnMaster::global().search("  ", 10).is_empty());
}

#[test]
#[cfg(feature = "hsn-catalog")]
fn test_a_code_known_without_a_rate_has_no_default_line() {
    // 28413000 (sodium dichromate) is listed, but neither it nor its headings has a checked rate.
    let master = HsnMaster::global();
    assert!(master.lookup("28413000").is_some());
    assert_eq!(master.default_rate("28413000"), None);
    assert!(
        matches!(line("28413000"), Err(InvoiceError::NoDefaultRate(code)) if code == "28413000")
    );
}

#[test]
fn test_subheading_rate_overrides_its_heading() {
    let master = HsnMaster::global();

    // paneer is nil-rated, other cheese is 5%
    assert_eq!(master.default_rate("04061000"), Some(BigDecimal::from(0)));
    assert_eq!(master.default_rate("04069000"), Some(BigDecimal::from(5)));
    // each cheese subheading has its own entry
    for code in ["040620", "040630", "040640", "040690"] {
        assert_eq!(master.lookup(code).unwrap().code, code);
        assert_eq!(master.default_rate(code), Some(BigDecimal::from(5)));
    }
    // sunglasses stay at 18%, corrective spectacles are 5%
    assert_eq!(master.default_rate("90041000"), Some(BigDecimal::from(18)));
    assert_eq!(master.default_rate("90049010"), Some(BigDecimal::from(5)));
}

#[test]
fn test_lookup_misses() {
    let master = HsnMaster::global();

    assert!(master.lookup("0000").is_none());
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
