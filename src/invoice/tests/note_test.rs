use crate::invoice::note::*;
use crate::invoice::types::{
    GstDocument, GstInvoice, GstLineItem, Gstin, InvoiceError, InvoiceNumberError, Recipient,
    StateCode, SupplyKind,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde_json::json;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER: &str = "27AAPFU0939F2ZU";

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn item(quantity: u32) -> GstLineItem {
    GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(quantity),
        BigDecimal::from(1000),
        BigDecimal::from(18),
    )
    .unwrap()
}

fn invoice_to(buyer: impl Into<Recipient>, quantity: u32) -> GstInvoice {
    GstInvoice::new(
        "INV-001",
        date(2024, 11, 15),
        Gstin::parse(SELLER).unwrap(),
        buyer,
        vec![item(quantity)],
    )
    .unwrap()
}

fn b2b_invoice() -> GstInvoice {
    invoice_to(Gstin::parse(BUYER).unwrap(), 10)
}

fn unregistered(state: &str) -> Recipient {
    Recipient::unregistered(StateCode::parse(state).unwrap())
}

#[test]
fn test_credit_note_takes_parties_and_original_from_its_invoice() {
    let invoice = b2b_invoice();
    let note = CreditNote::new(&invoice, "CN-001", date(2024, 11, 20), vec![item(2)]).unwrap();

    assert_eq!(note.note_number, "CN-001");
    assert_eq!(note.seller_gstin, invoice.seller_gstin);
    assert_eq!(note.buyer, invoice.buyer);
    assert_eq!(
        note.original,
        OriginalInvoice {
            number: "INV-001".to_string(),
            date: date(2024, 11, 15),
            kind: SupplyKind::B2b,
            value: BigDecimal::from(11800),
        }
    );
    assert_eq!(note.number(), "CN-001");
    assert_eq!(note.date(), date(2024, 11, 20));
}

#[test]
fn test_credit_note_records_the_supply_kind_of_its_invoice() {
    // ₹1,18,000 inter-state to an unregistered buyer is B2CL; ₹11,800 is B2CS
    let large = invoice_to(unregistered("29"), 100);
    let small = invoice_to(unregistered("29"), 10);

    let kind = |invoice: &GstInvoice| {
        CreditNote::new(invoice, "CN-001", date(2024, 11, 20), vec![item(1)])
            .unwrap()
            .original
            .kind
    };
    assert_eq!(kind(&large), SupplyKind::B2cl);
    assert_eq!(kind(&small), SupplyKind::B2cs);
}

#[test]
fn test_credit_note_is_priced_like_an_invoice_line() {
    let invoice = invoice_to(Gstin::parse("29AAPFU0939F1ZR").unwrap(), 10);
    let note = CreditNote::new(&invoice, "CN-001", date(2024, 11, 20), vec![item(2)]).unwrap();

    let breakdown = note.breakdown().unwrap();
    assert!(note.is_inter_state());
    assert_eq!(breakdown, item(2).breakdown(true).unwrap());
    assert_eq!(breakdown.igst, BigDecimal::from(360));
    assert_eq!(breakdown.total, BigDecimal::from(2360));
}

#[test]
fn test_credit_note_needs_a_valid_number_and_lines() {
    let invoice = b2b_invoice();
    let on = date(2024, 11, 20);

    let err = CreditNote::new(&invoice, "CN 001", on, vec![item(1)]).unwrap_err();
    assert!(matches!(
        err,
        InvoiceError::InvalidInvoiceNumber {
            reason: InvoiceNumberError::Characters,
            ..
        }
    ));
    let err = CreditNote::new(&invoice, "CN-001", on, Vec::new()).unwrap_err();
    assert!(matches!(err, InvoiceError::EmptyInvoice));
}

#[test]
fn test_credit_note_cannot_predate_its_invoice() {
    let err =
        CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 14), vec![item(1)]).unwrap_err();

    assert!(matches!(
        err,
        InvoiceError::NoteBeforeOriginal { note_date, original_date }
            if note_date == date(2024, 11, 14) && original_date == date(2024, 11, 15)
    ));
    // The same day is fine
    assert!(CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 15), vec![item(1)]).is_ok());
}

#[test]
fn test_credit_note_round_trips_through_json() {
    let note =
        CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 20), vec![item(2)]).unwrap();

    let value = serde_json::to_value(&note).unwrap();
    assert_eq!(
        value["original"],
        json!({"number": "INV-001", "date": "2024-11-15", "kind": "B2b", "value": "11800.00"})
    );
    let back: CreditNote = serde_json::from_value(value).unwrap();
    assert_eq!(back, note);
}

#[test]
fn test_deserialising_checks_the_note_like_construction() {
    let note =
        CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 20), vec![item(2)]).unwrap();
    let with = |key: &str, value: serde_json::Value| {
        let mut json = serde_json::to_value(&note).unwrap();
        json[key] = value;
        serde_json::from_value::<CreditNote>(json)
            .unwrap_err()
            .to_string()
    };

    assert!(with("note_date", json!("2024-11-01")).contains("before its invoice"));
    assert!(with("line_items", json!([])).contains("at least one line item"));
    assert!(with("note_number", json!("")).contains("invalid invoice number"));
    // A B2B original can't have gone to an unregistered buyer
    let err = with("buyer", json!({"unregistered": {"place_of_supply": "29"}}));
    assert!(err.contains("a B2B invoice can't have been issued to Unregistered"));
}

#[test]
fn test_b2cl_original_needs_an_unregistered_buyer_in_another_state() {
    let note = CreditNote::new(
        &invoice_to(unregistered("29"), 100),
        "CN-001",
        date(2024, 11, 20),
        vec![item(1)],
    )
    .unwrap();
    let mut json = serde_json::to_value(&note).unwrap();
    assert!(serde_json::from_value::<CreditNote>(json.clone()).is_ok());

    json["buyer"] = json!({"unregistered": {"place_of_supply": "27"}});
    let err = serde_json::from_value::<CreditNote>(json).unwrap_err();
    assert!(err.to_string().contains("a B2CL invoice"));
}

#[test]
fn test_credit_note_deadline_is_30_november_after_the_financial_year() {
    // FY 2024-25 runs April 2024 to March 2025
    assert_eq!(credit_note_deadline(date(2024, 4, 1)), date(2025, 11, 30));
    assert_eq!(credit_note_deadline(date(2024, 11, 15)), date(2025, 11, 30));
    assert_eq!(credit_note_deadline(date(2025, 3, 31)), date(2025, 11, 30));
    assert_eq!(credit_note_deadline(date(2025, 4, 1)), date(2026, 11, 30));

    let note =
        CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 20), vec![item(1)]).unwrap();
    assert_eq!(note.deadline(), date(2025, 11, 30));
}

#[test]
fn test_credit_note_cannot_credit_more_than_its_invoice() {
    // The invoice is 10 units: 11,800 with tax
    let invoice = b2b_invoice();
    let on = date(2024, 11, 20);

    let whole = CreditNote::new(&invoice, "CN-001", on, vec![item(10)]).unwrap();
    assert_eq!(whole.breakdown().unwrap().total, whole.original.value);

    let err = CreditNote::new(&invoice, "CN-002", on, vec![item(100)]).unwrap_err();
    assert!(matches!(
        err,
        InvoiceError::CreditExceedsInvoice { credited, invoice_value }
            if credited == 118_000 && invoice_value == 11_800
    ));
}

#[test]
fn test_deserialising_refuses_a_note_above_its_invoice_value() {
    let note =
        CreditNote::new(&b2b_invoice(), "CN-001", date(2024, 11, 20), vec![item(2)]).unwrap();
    let mut json = serde_json::to_value(&note).unwrap();
    json["original"]["value"] = json!("2359.99");

    let err = serde_json::from_value::<CreditNote>(json).unwrap_err();
    assert!(err
        .to_string()
        .contains("credit note total 2360.00 is more than its invoice's value 2359.99"));
}
