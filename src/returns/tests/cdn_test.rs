use crate::invoice::{CreditNote, GstInvoice, GstLineItem, Gstin, HsnMaster, Recipient, StateCode};
use crate::returns::cdn::*;
use crate::returns::gstr1::{Gstr1Error, Gstr1Return};
use crate::returns::period::ReturnPeriod;
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

fn line(quantity: &str, price: &str, rate: &str) -> GstLineItem {
    GstLineItem::new("998314", "Item", dec(quantity), dec(price), dec(rate)).unwrap()
}

fn invoice(number: &str, buyer: impl Into<Recipient>, lines: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(number, day(5), Gstin::parse(SELLER).unwrap(), buyer, lines).unwrap()
}

fn registered(gstin: &str) -> Recipient {
    Gstin::parse(gstin).unwrap().into()
}

fn unregistered(state: &str) -> Recipient {
    Recipient::unregistered(StateCode::parse(state).unwrap())
}

fn credit(invoice: &GstInvoice, number: &str, date: u32, lines: Vec<GstLineItem>) -> CreditNote {
    CreditNote::new(invoice, number, day(date), lines).unwrap()
}

fn build(invoices: &[GstInvoice], notes: &[CreditNote]) -> Result<Gstr1Return, Gstr1Error> {
    let period = ReturnPeriod::new(2024, 11).unwrap();
    Gstr1Return::build(
        &Gstin::parse(SELLER).unwrap(),
        period,
        invoices,
        notes,
        HsnMaster::global(),
    )
}

fn to_value(gstr1: &Gstr1Return) -> Value {
    serde_json::from_str(&gstr1.to_json().unwrap()).unwrap()
}

#[test]
fn test_credit_note_to_a_registered_buyer_is_reported_in_cdnr() {
    let sale = invoice(
        "INV-001",
        registered(BUYER_SAME_STATE),
        vec![line("10", "100", "18")],
    );
    let note = credit(&sale, "CN-001", 20, vec![line("2", "100", "18")]);

    let value = to_value(&build(&[sale], &[note]).unwrap());

    assert_eq!(
        value["cdnr"],
        json!([{
            "ctin": BUYER_SAME_STATE,
            "nt": [{
                "ntty": "C",
                "nt_num": "CN-001",
                "nt_dt": "20-11-2024",
                "pos": "27",
                "rchrg": "N",
                "inv_typ": "R",
                "val": 236.0,
                "itms": [{
                    "num": 1,
                    "itm_det": {
                        "rt": 18.0, "txval": 200.0, "iamt": 0.0,
                        "camt": 18.0, "samt": 18.0, "csamt": 0.0
                    }
                }]
            }]
        }])
    );
    assert!(value.get("cdnur").is_none());
}

#[test]
fn test_cdnr_groups_notes_by_buyer_in_gstin_order() {
    let other = invoice(
        "INV-001",
        registered(BUYER_OTHER_STATE),
        vec![line("1", "1000", "18")],
    );
    let same = invoice(
        "INV-002",
        registered(BUYER_SAME_STATE),
        vec![line("1", "1000", "18")],
    );
    let notes = [
        credit(&other, "CN-003", 25, vec![line("1", "100", "18")]),
        credit(&same, "CN-002", 22, vec![line("1", "100", "18")]),
        credit(&other, "CN-001", 21, vec![line("1", "100", "5")]),
    ];

    let gstr1 = build(&[other, same], &notes).unwrap();

    let parties: Vec<_> = gstr1
        .cdnr
        .iter()
        .map(|party| {
            let numbers: Vec<_> = party.notes.iter().map(|n| n.note_number.as_str()).collect();
            (party.buyer_gstin.as_str(), numbers)
        })
        .collect();
    assert_eq!(
        parties,
        [
            (BUYER_SAME_STATE, vec!["CN-002"]),
            (BUYER_OTHER_STATE, vec!["CN-001", "CN-003"]),
        ]
    );
    // Inter-state notes carry IGST, and the value includes it
    let note = &gstr1.cdnr[1].notes[0];
    assert_eq!(note.items[0].detail.igst, dec("5"));
    assert_eq!(note.note_value, dec("105"));
}

#[test]
fn test_credit_note_against_a_b2cl_invoice_is_reported_in_cdnur() {
    // ₹1,18,000 inter-state to an unregistered buyer is B2CL
    let sale = invoice(
        "INV-001",
        unregistered("29"),
        vec![line("100", "1000", "18")],
    );
    let note = credit(&sale, "CN-001", 20, vec![line("10", "1000", "18")]);

    let gstr1 = build(&[sale], &[note]).unwrap();
    let value = to_value(&gstr1);

    assert_eq!(gstr1.cdnur[0].original_type, CdnurType::B2cl);
    assert_eq!(
        value["cdnur"],
        json!([{
            "typ": "B2CL",
            "ntty": "C",
            "nt_num": "CN-001",
            "nt_dt": "20-11-2024",
            "pos": "29",
            "val": 11800.0,
            "itms": [{
                "num": 1,
                "itm_det": { "rt": 18.0, "txval": 10000.0, "iamt": 1800.0, "csamt": 0.0 }
            }]
        }])
    );
    assert!(gstr1.cdnr.is_empty());
}

#[test]
fn test_credit_note_against_a_b2cs_invoice_has_no_table_9b_entry() {
    let sale = invoice("INV-001", unregistered("27"), vec![line("10", "100", "18")]);
    let note = credit(&sale, "CN-001", 20, vec![line("1", "100", "18")]);

    let gstr1 = build(&[sale], &[note]).unwrap();

    assert!(gstr1.cdnr.is_empty());
    assert!(gstr1.cdnur.is_empty());
}

#[test]
fn test_credit_note_lists_only_its_taxed_rates_and_keeps_its_whole_value() {
    let exempt = GstLineItem::exempt("998314", "Item", dec("1"), dec("50")).unwrap();
    let sale = invoice(
        "INV-001",
        registered(BUYER_SAME_STATE),
        vec![line("1", "1000", "18"), exempt.clone()],
    );
    let mixed = credit(
        &sale,
        "CN-001",
        20,
        vec![line("1", "100", "18"), exempt.clone()],
    );
    let untaxed = credit(&sale, "CN-002", 21, vec![exempt]);

    let gstr1 = build(&[sale], &[mixed, untaxed]).unwrap();

    // The note with no taxed line is left out of Table 9B
    let notes = &gstr1.cdnr[0].notes;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].items.len(), 1);
    assert_eq!(notes[0].note_value, dec("168"));
}
