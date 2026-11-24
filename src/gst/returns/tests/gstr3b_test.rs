use crate::gst::invoice::{GstCreditNote, GstInvoice, GstLineItem, Gstin, SupplyTreatment};
use crate::gst::returns::gstr1::Gstr1Error;
use crate::gst::returns::gstr3b::*;
use crate::gst::returns::period::ReturnPeriod;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
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
