use crate::invoice::validation::ComplianceIssue;
use crate::invoice::{GstInvoice, GstLineItem, Gstin, HsnMaster};
use crate::returns::gstr1::*;
use crate::returns::period::ReturnPeriod;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::str::FromStr;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

fn gstin(value: &str) -> Gstin {
    Gstin::parse(value).unwrap()
}

fn dec(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).unwrap()
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 11, d).unwrap()
}

fn period() -> ReturnPeriod {
    ReturnPeriod::new(2024, 11).unwrap()
}

fn line(hsn: &str, quantity: &str, price: &str, rate: &str) -> GstLineItem {
    GstLineItem::new(hsn, "Item", dec(quantity), dec(price), dec(rate)).unwrap()
}

fn invoice(number: &str, date: NaiveDate, buyer: &str, lines: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(number, date, gstin(SELLER), gstin(buyer), lines).unwrap()
}

fn build(invoices: &[GstInvoice]) -> Result<Gstr1Return, Gstr1Error> {
    Gstr1Return::build(&gstin(SELLER), period(), invoices, HsnMaster::global())
}

fn to_value(gstr1: &Gstr1Return) -> Value {
    serde_json::from_str(&gstr1.to_json().unwrap()).unwrap()
}

#[test]
fn test_single_intra_state_invoice() {
    let gstr1 = build(&[invoice(
        "INV-001",
        day(5),
        BUYER_SAME_STATE,
        vec![line("998314", "2", "500", "18")],
    )])
    .unwrap();

    assert_eq!(gstr1.b2b.len(), 1);
    let inv = &gstr1.b2b[0].invoices[0];
    assert_eq!(gstr1.b2b[0].buyer_gstin, gstin(BUYER_SAME_STATE));
    assert_eq!(inv.invoice_value, dec("1180"));
    assert_eq!(inv.place_of_supply, "27");
    let detail = &inv.items[0].detail;
    assert_eq!(detail.taxable_value, dec("1000"));
    assert_eq!(detail.cgst, dec("90"));
    assert_eq!(detail.sgst, dec("90"));
    assert_eq!(detail.igst, dec("0"));
}

#[test]
fn test_inter_state_invoice_is_charged_igst() {
    let gstr1 = build(&[invoice(
        "INV-001",
        day(5),
        BUYER_OTHER_STATE,
        vec![line("998314", "1", "1000", "18")],
    )])
    .unwrap();

    let inv = &gstr1.b2b[0].invoices[0];
    assert_eq!(inv.place_of_supply, "29");
    assert_eq!(inv.items[0].detail.igst, dec("180"));
    assert_eq!(inv.items[0].detail.cgst, dec("0"));
}

#[test]
fn test_invoices_to_the_same_buyer_share_one_entry() {
    let gstr1 = build(&[
        invoice(
            "INV-002",
            day(9),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "100", "18")],
        ),
        invoice(
            "INV-001",
            day(3),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "200", "18")],
        ),
    ])
    .unwrap();

    assert_eq!(gstr1.b2b.len(), 1);
    let numbers: Vec<_> = gstr1.b2b[0]
        .invoices
        .iter()
        .map(|inv| inv.invoice_number.as_str())
        .collect();
    assert_eq!(numbers, ["INV-001", "INV-002"]);
}

#[test]
fn test_different_buyers_get_separate_entries_in_gstin_order() {
    let gstr1 = build(&[
        invoice(
            "INV-001",
            day(1),
            BUYER_OTHER_STATE,
            vec![line("998314", "1", "100", "18")],
        ),
        invoice(
            "INV-002",
            day(2),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "100", "18")],
        ),
    ])
    .unwrap();

    let buyers: Vec<_> = gstr1.b2b.iter().map(|p| p.buyer_gstin.as_str()).collect();
    assert_eq!(buyers, [BUYER_SAME_STATE, BUYER_OTHER_STATE]);
}

#[test]
fn test_lines_are_grouped_by_rate_within_an_invoice() {
    let gstr1 = build(&[invoice(
        "INV-001",
        day(1),
        BUYER_SAME_STATE,
        vec![
            line("998314", "1", "1000", "18"),
            line("1905", "10", "20", "5"),
            line("8471", "1", "500", "18.00"),
        ],
    )])
    .unwrap();

    let items = &gstr1.b2b[0].invoices[0].items;
    assert_eq!(items.len(), 2);
    assert_eq!((items[0].number, &items[0].detail.rate), (1, &dec("5")));
    assert_eq!(items[0].detail.taxable_value, dec("200"));
    assert_eq!((items[1].number, &items[1].detail.rate), (2, &dec("18")));
    assert_eq!(items[1].detail.taxable_value, dec("1500"));
    assert_eq!(items[1].detail.cgst, dec("135"));
}

#[test]
fn test_zero_rated_supply_reports_rate_zero_and_no_tax() {
    let gstr1 = build(&[invoice(
        "INV-001",
        day(1),
        BUYER_OTHER_STATE,
        vec![line("4901", "4", "250", "0")],
    )])
    .unwrap();

    let inv = &gstr1.b2b[0].invoices[0];
    assert_eq!(inv.invoice_value, dec("1000"));
    assert_eq!(inv.items[0].detail.rate, dec("0"));
    assert_eq!(inv.items[0].detail.igst, dec("0"));

    let value = to_value(&gstr1);
    assert_eq!(
        value["b2b"][0]["inv"][0]["itms"][0]["itm_det"]["rt"],
        json!(0.0)
    );
    assert_eq!(value["hsn"]["hsn_b2b"][0]["txval"], json!(1000.0));
}

#[test]
fn test_hsn_summary_groups_by_code_and_rate_across_invoices() {
    let gstr1 = build(&[
        invoice(
            "INV-001",
            day(1),
            BUYER_SAME_STATE,
            vec![line("998314", "2", "500", "18")],
        ),
        invoice(
            "INV-002",
            day(2),
            BUYER_OTHER_STATE,
            vec![
                line("998314", "3", "100", "18"),
                line("998314", "1", "100", "12"),
            ],
        ),
    ])
    .unwrap();

    let rows = &gstr1.hsn.b2b;
    assert_eq!(rows.len(), 2);
    let (low, high) = (&rows[0], &rows[1]);
    assert_eq!((low.number, &low.rate), (1, &dec("12")));
    assert_eq!((high.number, &high.rate), (2, &dec("18")));
    assert_eq!(high.hsn_sac, "998314");
    assert_eq!(high.description, "IT design and development services");
    assert_eq!(high.uqc, SERVICES_UQC);
    assert_eq!(high.quantity, dec("0"));
    assert_eq!(high.taxable_value, dec("1300"));
    assert_eq!(high.cgst, dec("90"));
    assert_eq!(high.sgst, dec("90"));
    assert_eq!(high.igst, dec("54"));
    assert_eq!(high.total_value, dec("1534"));
}

#[test]
fn test_goods_report_their_quantity_and_services_report_na_with_zero() {
    let gstr1 = build(&[invoice(
        "INV-001",
        day(1),
        BUYER_SAME_STATE,
        vec![
            line("1905", "10", "20", "5"),
            line("1905", "2.5", "20", "5"),
            line("998314", "3", "100", "18"),
        ],
    )])
    .unwrap();

    let (goods, services) = (&gstr1.hsn.b2b[0], &gstr1.hsn.b2b[1]);
    assert_eq!(
        (goods.hsn_sac.as_str(), goods.uqc.as_str()),
        ("1905", GOODS_UQC)
    );
    assert_eq!(goods.quantity, dec("12.5"));
    assert_eq!(
        (services.hsn_sac.as_str(), services.uqc.as_str()),
        ("998314", SERVICES_UQC)
    );
    assert_eq!(services.quantity, dec("0"));
    assert_eq!(services.taxable_value, dec("300"));

    let value = to_value(&gstr1);
    assert_eq!(value["hsn"]["hsn_b2b"][0]["uqc"], json!("OTH"));
    assert_eq!(value["hsn"]["hsn_b2b"][0]["qty"], json!(12.5));
    assert_eq!(value["hsn"]["hsn_b2b"][1]["uqc"], json!("NA"));
    assert_eq!(value["hsn"]["hsn_b2b"][1]["qty"], json!(0.0));
}

#[test]
fn test_hsn_row_falls_back_to_the_line_description_for_unknown_codes() {
    let item =
        GstLineItem::new("99999999", "Bespoke widget", dec("1"), dec("10"), dec("18")).unwrap();
    let gstr1 = build(&[invoice("INV-001", day(1), BUYER_SAME_STATE, vec![item])]).unwrap();
    assert_eq!(gstr1.hsn.b2b[0].description, "Bespoke widget");
}

#[test]
fn test_document_summary_spans_first_to_last_invoice() {
    let gstr1 = build(&[
        invoice(
            "INV-003",
            day(20),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "1", "18")],
        ),
        invoice(
            "INV-001",
            day(1),
            BUYER_OTHER_STATE,
            vec![line("998314", "1", "1", "18")],
        ),
        invoice(
            "INV-002",
            day(10),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "1", "18")],
        ),
    ])
    .unwrap();

    let summary = &gstr1.doc_issue.documents[0];
    assert_eq!(summary.doc_type, OUTWARD_INVOICES_DOC_TYPE);
    let series = &summary.series[0];
    assert_eq!(
        (series.from.as_str(), series.to.as_str()),
        ("INV-001", "INV-003")
    );
    assert_eq!(
        (series.total, series.cancelled, series.net_issued),
        (3, 0, 3)
    );
}

#[test]
fn test_no_invoices_gives_an_empty_return() {
    let gstr1 = build(&[]).unwrap();
    assert!(gstr1.b2b.is_empty() && gstr1.hsn.is_empty() && gstr1.doc_issue.is_empty());
    assert_eq!(to_value(&gstr1), json!({ "gstin": SELLER, "fp": "112024" }));
}

#[test]
fn test_invoice_from_another_seller_is_rejected() {
    let other = GstInvoice::new(
        "INV-001",
        day(1),
        gstin(BUYER_SAME_STATE),
        gstin(SELLER),
        vec![line("998314", "1", "1", "18")],
    )
    .unwrap();
    assert!(matches!(
        build(&[other]),
        Err(Gstr1Error::SellerMismatch { invoice_number, seller })
            if invoice_number == "INV-001" && seller == gstin(BUYER_SAME_STATE)
    ));
}

#[test]
fn test_invoice_outside_the_period_is_rejected() {
    let october = NaiveDate::from_ymd_opt(2024, 10, 31).unwrap();
    let inv = invoice(
        "INV-001",
        october,
        BUYER_SAME_STATE,
        vec![line("998314", "1", "1", "18")],
    );
    assert!(matches!(
        build(&[inv]),
        Err(Gstr1Error::OutsidePeriod { date, .. }) if date == october
    ));
}

#[test]
fn test_duplicate_invoice_numbers_are_rejected_ignoring_case() {
    let lines = || vec![line("998314", "1", "1", "18")];
    assert!(matches!(
        build(&[
            invoice("INV-A1", day(1), BUYER_SAME_STATE, lines()),
            invoice("inv-a1", day(2), BUYER_OTHER_STATE, lines()),
        ]),
        Err(Gstr1Error::DuplicateInvoiceNumber(number)) if number == "inv-a1"
    ));
}

#[test]
fn test_non_compliant_invoice_is_rejected_with_its_errors() {
    let mut inv = invoice(
        "INV-001",
        day(1),
        BUYER_SAME_STATE,
        vec![line("998314", "1", "1", "18")],
    );
    inv.buyer_gstin = gstin(SELLER);
    assert!(matches!(
        build(&[inv]),
        Err(Gstr1Error::NotCompliant { issues, .. })
            if issues == [ComplianceIssue::SameSellerAndBuyer(SELLER.to_string())]
    ));
}

#[test]
fn test_warnings_do_not_block_the_return() {
    // 998314 defaults to 18%, so 12% is only a warning
    assert!(build(&[invoice(
        "INV-001",
        day(1),
        BUYER_SAME_STATE,
        vec![line("998314", "1", "1", "12")]
    )])
    .is_ok());
}

#[test]
fn test_json_uses_portal_keys_and_numeric_amounts() {
    let gstr1 = build(&[invoice(
        "INV/24-25/001",
        day(15),
        BUYER_OTHER_STATE,
        vec![line("998314", "3", "333.335", "18")],
    )])
    .unwrap();

    let value = to_value(&gstr1);
    assert_eq!(
        value["b2b"][0],
        json!({
            "ctin": BUYER_OTHER_STATE,
            "inv": [{
                "inum": "INV/24-25/001",
                "idt": "15-11-2024",
                "val": 1180.01,
                "pos": "29",
                "rchrg": "N",
                "inv_typ": "R",
                "itms": [{
                    "num": 1,
                    "itm_det": {
                        "rt": 18.0,
                        "txval": 1000.01,
                        "iamt": 180.0,
                        "camt": 0.0,
                        "samt": 0.0,
                        "csamt": 0.0
                    }
                }]
            }]
        })
    );
    assert_eq!(
        value["hsn"]["hsn_b2b"][0],
        json!({
            "num": 1,
            "hsn_sc": "998314",
            "desc": "IT design and development services",
            "uqc": "NA",
            "qty": 0.0,
            "val": 1180.01,
            "txval": 1000.01,
            "iamt": 180.0,
            "camt": 0.0,
            "samt": 0.0,
            "csamt": 0.0,
            "rt": 18.0
        })
    );
    assert_eq!(
        value["doc_issue"]["doc_det"][0],
        json!({
            "doc_num": 1,
            "doc_typ": OUTWARD_INVOICES_DOC_TYPE,
            "docs": [{
                "num": 1,
                "from": "INV/24-25/001",
                "to": "INV/24-25/001",
                "totnum": 1,
                "cancel": 0,
                "net_issue": 1
            }]
        })
    );
    assert_eq!(value["gstin"], json!(SELLER));
    assert_eq!(value["fp"], json!("112024"));
}

#[test]
fn test_output_does_not_depend_on_input_order() {
    let invoices = [
        invoice(
            "INV-002",
            day(2),
            BUYER_OTHER_STATE,
            vec![line("1905", "1", "10", "5")],
        ),
        invoice(
            "INV-001",
            day(1),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "10", "18")],
        ),
        invoice(
            "INV-003",
            day(2),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "10", "18")],
        ),
    ];
    let reversed: Vec<_> = invoices.iter().rev().cloned().collect();
    assert_eq!(build(&invoices).unwrap(), build(&reversed).unwrap());
}

#[test]
fn test_invoice_numbers_order_by_their_numeric_suffix() {
    let gstr1 = build(&[
        invoice(
            "INV-10",
            day(30),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "1", "18")],
        ),
        invoice(
            "INV-9",
            day(30),
            BUYER_SAME_STATE,
            vec![line("998314", "1", "1", "18")],
        ),
    ])
    .unwrap();

    let series = &gstr1.doc_issue.documents[0].series[0];
    assert_eq!(
        (series.from.as_str(), series.to.as_str()),
        ("INV-9", "INV-10")
    );
    let numbers: Vec<&str> = gstr1.b2b[0]
        .invoices
        .iter()
        .map(|inv| inv.invoice_number.as_str())
        .collect();
    assert_eq!(numbers, ["INV-9", "INV-10"]);
}
