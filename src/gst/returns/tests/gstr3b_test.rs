use crate::gst::invoice::{
    GstCreditNote, GstInvoice, GstLineItem, Gstin, Recipient, StateCode, SupplyTreatment,
};
use crate::gst::returns::gstr1::Gstr1Error;
use crate::gst::returns::gstr3b::*;
use crate::gst::returns::period::ReturnPeriod;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::str::FromStr;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

fn dec(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).unwrap()
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 11, d).unwrap()
}

fn line(price: &str, rate: &str) -> GstLineItem {
    GstLineItem::new("998314", "Item", dec("1"), dec(price), dec(rate)).unwrap()
}

fn treated(price: &str, treatment: SupplyTreatment) -> GstLineItem {
    GstLineItem {
        treatment,
        ..line(price, "0")
    }
}

fn invoice(number: &str, buyer: &str, lines: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        number,
        day(5),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(buyer).unwrap(),
        lines,
    )
    .unwrap()
}

/// An invoice to an unregistered buyer receiving the supply in `place_of_supply`
fn b2c_invoice(number: &str, place_of_supply: &str, lines: Vec<GstLineItem>) -> GstInvoice {
    let buyer = Recipient::unregistered(StateCode::parse(place_of_supply).unwrap());
    GstInvoice::new(number, day(5), Gstin::parse(SELLER).unwrap(), buyer, lines).unwrap()
}

fn inter_row(place_of_supply: &str, taxable_value: &str, igst: &str) -> InterStateRow {
    InterStateRow {
        place_of_supply: place_of_supply.to_string(),
        taxable_value: dec(taxable_value),
        igst: dec(igst),
    }
}

fn to_value(summary: &Gstr3bOutward) -> Value {
    serde_json::from_str(&summary.to_json().unwrap()).unwrap()
}

fn build(invoices: &[GstInvoice], notes: &[GstCreditNote]) -> Result<Gstr3bOutward, Gstr1Error> {
    let period = ReturnPeriod::new(2024, 11).unwrap();
    Gstr3bOutward::build(&Gstin::parse(SELLER).unwrap(), period, invoices, notes)
}

fn row(taxable_value: &str, igst: &str, cgst: &str, sgst: &str) -> OutwardRow {
    OutwardRow {
        taxable_value: dec(taxable_value),
        igst: dec(igst),
        cgst: dec(cgst),
        sgst: dec(sgst),
        cess: dec("0"),
    }
}

#[test]
fn test_taxed_lines_report_in_row_a_with_their_tax() {
    let summary = build(
        &[
            invoice("INV-001", BUYER_SAME_STATE, vec![line("1000", "18")]),
            invoice("INV-002", BUYER_OTHER_STATE, vec![line("500", "5")]),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(summary.outward.taxable, row("1500", "25", "90", "90"));
    assert_eq!(summary.outward.zero_rated, OutwardRow::default());
    assert_eq!(summary.outward.reverse_charge, OutwardRow::default());
}

#[test]
fn test_nil_rated_and_exempt_lines_report_in_row_c_and_non_gst_in_row_e() {
    let lines = vec![
        line("1000", "18"),
        line("200", "0"),
        treated("300", SupplyTreatment::Exempt),
        treated("400", SupplyTreatment::NonGst),
    ];
    let summary = build(&[invoice("INV-001", BUYER_SAME_STATE, lines)], &[]).unwrap();

    assert_eq!(summary.outward.taxable.taxable_value, dec("1000"));
    assert_eq!(summary.outward.nil_rated_exempt.taxable_value, dec("500"));
    assert_eq!(summary.outward.non_gst.taxable_value, dec("400"));
}

#[test]
fn test_a_credit_note_is_subtracted_from_its_row() {
    let sale = invoice("INV-001", BUYER_SAME_STATE, vec![line("1000", "18")]);
    let note = GstCreditNote::new(&sale, "CN-001", day(9), vec![line("100", "18")]).unwrap();

    let summary = build(&[sale], &[note]).unwrap();

    assert_eq!(summary.outward.taxable, row("900", "0", "81", "81"));
}

#[test]
fn test_documents_are_checked_as_gstr1_checks_them() {
    let mut late = invoice("INV-001", BUYER_SAME_STATE, vec![line("1000", "18")]);
    late.invoice_date = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();

    assert!(matches!(
        build(&[late], &[]),
        Err(Gstr1Error::OutsidePeriod { .. })
    ));
}

#[test]
fn test_inter_state_b2cs_invoice_listed_under_its_place_of_supply() {
    let summary = build(
        &[b2c_invoice("INV-001", "29", vec![line("500", "18")])],
        &[],
    )
    .unwrap();

    assert_eq!(
        summary.inter_state_unregistered,
        [inter_row("29", "500", "90")]
    );
}

#[test]
fn test_intra_state_and_b2b_supplies_not_in_table_3_2() {
    let summary = build(
        &[
            b2c_invoice("INV-001", "27", vec![line("500", "18")]),
            invoice("INV-002", BUYER_OTHER_STATE, vec![line("1000", "18")]),
        ],
        &[],
    )
    .unwrap();

    assert!(summary.inter_state_unregistered.is_empty());
    assert_eq!(summary.outward.taxable, row("1500", "180", "45", "45"));
}

#[test]
fn test_credit_note_reduces_its_places_inter_state_row() {
    let sale = b2c_invoice("INV-001", "29", vec![line("300000", "18")]);
    let note = GstCreditNote::new(&sale, "CN-001", day(9), vec![line("100000", "18")]).unwrap();

    let summary = build(&[sale], &[note]).unwrap();

    assert_eq!(
        summary.inter_state_unregistered,
        [inter_row("29", "200000", "36000")]
    );
}

#[test]
fn test_nil_rated_line_left_out_of_table_3_2() {
    let lines = vec![
        line("500", "18"),
        line("200", "0"),
        treated("300", SupplyTreatment::Exempt),
    ];
    let summary = build(&[b2c_invoice("INV-001", "29", lines)], &[]).unwrap();

    assert_eq!(
        summary.inter_state_unregistered,
        [inter_row("29", "500", "90")]
    );
}

#[test]
fn test_inter_state_rows_summed_per_place_and_ordered_by_state_code() {
    let summary = build(
        &[
            b2c_invoice("INV-001", "29", vec![line("500", "18")]),
            b2c_invoice("INV-002", "07", vec![line("100", "5")]),
            b2c_invoice("INV-003", "29", vec![line("200", "12")]),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(
        summary.inter_state_unregistered,
        [inter_row("07", "100", "5"), inter_row("29", "700", "114")]
    );
}

#[test]
fn test_json_uses_portal_keys_and_numeric_amounts() {
    let summary = build(
        &[
            invoice("INV-001", BUYER_SAME_STATE, vec![line("1000", "18")]),
            b2c_invoice("INV-002", "29", vec![line("33.33", "5"), line("200", "0")]),
            invoice(
                "INV-003",
                BUYER_SAME_STATE,
                vec![treated("400", SupplyTreatment::NonGst)],
            ),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(
        to_value(&summary),
        json!({
            "gstin": SELLER,
            "ret_period": "112024",
            "sup_details": {
                "osup_det": {
                    "txval": 1033.33, "iamt": 1.67, "camt": 90.0, "samt": 90.0, "csamt": 0.0
                },
                "osup_zero": { "txval": 0.0, "iamt": 0.0, "csamt": 0.0 },
                "osup_nil_exmp": { "txval": 200.0 },
                "isup_rev": { "txval": 0.0, "iamt": 0.0, "camt": 0.0, "samt": 0.0, "csamt": 0.0 },
                "osup_nongst": { "txval": 400.0 }
            },
            "inter_sup": {
                "unreg_details": [{ "pos": "29", "txval": 33.33, "iamt": 1.67 }],
                "comp_details": [],
                "uin_details": []
            }
        })
    );
}
