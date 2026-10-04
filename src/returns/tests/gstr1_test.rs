use crate::invoice::validation::ComplianceIssue;
use crate::invoice::{GstInvoice, GstLineItem, Gstin, HsnMaster, Recipient, StateCode, SupplyKind};
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
    inv.buyer = gstin(SELLER).into();
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

/// An invoice to an unregistered buyer receiving the supply in `place_of_supply`
fn b2c_invoice(
    number: &str,
    date: NaiveDate,
    place_of_supply: &str,
    lines: Vec<GstLineItem>,
) -> GstInvoice {
    let buyer = Recipient::unregistered(StateCode::parse(place_of_supply).unwrap());
    GstInvoice::new(number, date, gstin(SELLER), buyer, lines).unwrap()
}

#[test]
fn test_b2cl_invoices_are_grouped_by_place_of_supply() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-003",
            day(20),
            "29",
            vec![line("998314", "1", "200000", "18")],
        ),
        invoice(
            "INV-001",
            day(5),
            BUYER_SAME_STATE,
            vec![line("998314", "2", "500", "18")],
        ),
        b2c_invoice(
            "INV-002",
            day(10),
            "07",
            vec![
                line("847130", "2", "55000", "18"),
                line("1905", "100", "12.50", "5"),
            ],
        ),
        b2c_invoice(
            "INV-004",
            day(25),
            "29",
            vec![line("998314", "1", "150000", "18")],
        ),
    ])
    .unwrap();

    assert_eq!(gstr1.b2b.len(), 1);
    assert_eq!(gstr1.b2b[0].invoices.len(), 1);

    let places: Vec<_> = gstr1
        .b2cl
        .iter()
        .map(|p| p.place_of_supply.as_str())
        .collect();
    assert_eq!(places, ["07", "29"]);
    let numbers: Vec<_> = gstr1.b2cl[1]
        .invoices
        .iter()
        .map(|i| i.invoice_number.as_str())
        .collect();
    assert_eq!(numbers, ["INV-003", "INV-004"]);

    let delhi = &gstr1.b2cl[0].invoices[0];
    assert_eq!(delhi.invoice_value, dec("131112.50"));
    let rates: Vec<_> = delhi.items.iter().map(|i| i.detail.rate.clone()).collect();
    assert_eq!(rates, [dec("5"), dec("18")]);
    assert_eq!(delhi.items[0].detail.taxable_value, dec("1250"));
    assert_eq!(delhi.items[0].detail.igst, dec("62.50"));
    assert_eq!(delhi.items[1].detail.igst, dec("19800"));
}

#[test]
fn test_hsn_summary_splits_b2b_and_b2c_tabs() {
    let gstr1 = build(&[
        invoice(
            "INV-001",
            day(5),
            BUYER_OTHER_STATE,
            vec![line("998314", "2", "500", "18")],
        ),
        b2c_invoice(
            "INV-002",
            day(10),
            "29",
            vec![line("998314", "1", "200000", "18")],
        ),
    ])
    .unwrap();

    assert_eq!(gstr1.hsn.b2b.len(), 1);
    assert_eq!(gstr1.hsn.b2b[0].taxable_value, dec("1000"));
    assert_eq!(gstr1.hsn.b2c.len(), 1);
    assert_eq!(gstr1.hsn.b2c[0].number, 1);
    assert_eq!(gstr1.hsn.b2c[0].taxable_value, dec("200000"));
    assert_eq!(gstr1.hsn.b2c[0].igst, dec("36000"));
}

#[test]
fn test_doc_issue_counts_b2b_and_b2cl_invoices_as_one_series() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-002",
            day(10),
            "29",
            vec![line("998314", "1", "200000", "18")],
        ),
        invoice(
            "INV-001",
            day(5),
            BUYER_SAME_STATE,
            vec![line("998314", "2", "500", "18")],
        ),
    ])
    .unwrap();

    let series = &gstr1.doc_issue.documents[0].series[0];
    assert_eq!(
        (series.from.as_str(), series.to.as_str()),
        ("INV-001", "INV-002")
    );
    assert_eq!(series.total, 2);
}

#[test]
fn test_b2cs_invoices_are_reported_in_table_7() {
    let small = b2c_invoice(
        "INV-001",
        day(5),
        "29",
        vec![line("998314", "1", "1000", "18")],
    );
    let intra = b2c_invoice(
        "INV-002",
        day(5),
        "27",
        vec![line("998314", "1", "500000", "18")],
    );
    for invoice in [&small, &intra] {
        assert_eq!(invoice.supply_kind().unwrap(), SupplyKind::B2cs);
    }

    let gstr1 = build(&[small, intra]).unwrap();
    assert!(gstr1.b2b.is_empty());
    assert!(gstr1.b2cl.is_empty());

    let rows: Vec<_> = gstr1
        .b2cs
        .iter()
        .map(|r| (r.place_of_supply.as_str(), r.supply_type))
        .collect();
    assert_eq!(rows, [("27", SupplyType::Intra), ("29", SupplyType::Inter)]);

    let intra = &gstr1.b2cs[0];
    assert_eq!(intra.taxable_value, dec("500000"));
    assert_eq!(intra.cgst, Some(dec("45000")));
    assert_eq!(intra.sgst, Some(dec("45000")));
    assert_eq!(intra.igst, None);
    let inter = &gstr1.b2cs[1];
    assert_eq!(inter.igst, Some(dec("180")));
    assert_eq!((inter.cgst.clone(), inter.sgst.clone()), (None, None));
}

#[test]
fn test_b2cs_rows_sum_invoices_per_place_and_rate() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-003",
            day(20),
            "27",
            vec![line("1905", "10", "12.50", "5")],
        ),
        b2c_invoice(
            "INV-001",
            day(5),
            "27",
            vec![
                line("998314", "2", "500", "18"),
                line("1905", "4", "10", "5"),
            ],
        ),
        b2c_invoice(
            "INV-002",
            day(9),
            "07",
            vec![line("998314", "1", "2000", "18")],
        ),
        b2c_invoice(
            "INV-004",
            day(25),
            "27",
            vec![line("998314", "1", "300", "18.00")],
        ),
    ])
    .unwrap();

    let rows: Vec<_> = gstr1
        .b2cs
        .iter()
        .map(|r| {
            (
                r.place_of_supply.as_str(),
                r.rate.clone(),
                r.taxable_value.clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("07", dec("18"), dec("2000")),
            ("27", dec("5"), dec("165")),
            ("27", dec("18"), dec("1300")),
        ]
    );
    assert_eq!(gstr1.b2cs[1].cgst, Some(dec("4.13")));
    assert_eq!(gstr1.b2cs[2].sgst, Some(dec("117")));
}

#[test]
fn test_b2cs_json_uses_portal_keys() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-001",
            day(5),
            "27",
            vec![line("998314", "1", "1000", "18")],
        ),
        b2c_invoice(
            "INV-002",
            day(6),
            "29",
            vec![line("998314", "1", "2000", "18")],
        ),
    ])
    .unwrap();
    let value = to_value(&gstr1);

    assert_eq!(
        value["b2cs"],
        json!([
            {
                "sply_ty": "INTRA", "rt": 18.0, "typ": "OE", "pos": "27",
                "txval": 1000.0, "camt": 90.0, "samt": 90.0, "csamt": 0.0
            },
            {
                "sply_ty": "INTER", "rt": 18.0, "typ": "OE", "pos": "29",
                "txval": 2000.0, "iamt": 360.0, "csamt": 0.0
            }
        ])
    );
    assert!(value.get("b2cl").is_none());
}

#[test]
fn test_b2c_hsn_tab_and_documents_cover_b2cl_and_b2cs() {
    let gstr1 = build(&[
        invoice(
            "INV-001",
            day(5),
            BUYER_OTHER_STATE,
            vec![line("998314", "2", "500", "18")],
        ),
        b2c_invoice(
            "INV-002",
            day(10),
            "29",
            vec![line("998314", "1", "200000", "18")],
        ),
        b2c_invoice(
            "INV-003",
            day(12),
            "27",
            vec![line("998314", "1", "1000", "18")],
        ),
    ])
    .unwrap();

    assert_eq!(gstr1.hsn.b2b[0].taxable_value, dec("1000"));
    assert_eq!(gstr1.hsn.b2c.len(), 1);
    assert_eq!(gstr1.hsn.b2c[0].taxable_value, dec("201000"));
    assert_eq!(gstr1.hsn.b2c[0].igst, dec("36000"));
    assert_eq!(gstr1.hsn.b2c[0].cgst, dec("90"));

    let series = &gstr1.doc_issue.documents[0].series[0];
    assert_eq!(
        (series.from.as_str(), series.to.as_str(), series.total),
        ("INV-001", "INV-003", 3)
    );
}

#[test]
fn test_b2cl_json_uses_portal_keys() {
    let gstr1 = build(&[b2c_invoice(
        "INV-001",
        day(15),
        "29",
        vec![line("998314", "3", "40000.005", "18")],
    )])
    .unwrap();
    let value = to_value(&gstr1);

    assert!(value.get("b2b").is_none());
    assert_eq!(
        value["b2cl"],
        json!([{
            "pos": "29",
            "inv": [{
                "inum": "INV-001",
                "idt": "15-11-2024",
                "val": 141600.02,
                "itms": [{
                    "num": 1,
                    "itm_det": { "rt": 18.0, "txval": 120000.02, "iamt": 21600.0, "csamt": 0.0 }
                }]
            }]
        }])
    );
    assert!(value["hsn"].get("hsn_b2b").is_none());
    assert_eq!(value["hsn"]["hsn_b2c"][0]["hsn_sc"], "998314");
    assert_eq!(value["hsn"]["hsn_b2c"][0]["uqc"], "NA");
}

#[test]
fn test_b2cs_zero_rated_lines_go_to_table_8_not_table_7() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-001",
            day(5),
            "27",
            vec![
                line("998314", "1", "1000", "18"),
                line("4901", "10", "150", "0"),
            ],
        ),
        b2c_invoice(
            "INV-002",
            day(6),
            "27",
            vec![line("4901", "2", "100", "0.00")],
        ),
        b2c_invoice("INV-003", day(7), "29", vec![line("4901", "4", "250", "0")]),
    ])
    .unwrap();

    // Only the taxable line is a Table 7 row; no rt 0 rows
    let rows: Vec<_> = gstr1
        .b2cs
        .iter()
        .map(|r| (r.place_of_supply.as_str(), r.rate.clone()))
        .collect();
    assert_eq!(rows, [("27", dec("18"))]);

    let nil: Vec<_> = gstr1
        .nil
        .rows
        .iter()
        .map(|r| (r.supply_type, r.nil_rated.clone()))
        .collect();
    assert_eq!(
        nil,
        [
            (NilSupplyType::InterB2c, dec("1000")),
            (NilSupplyType::IntraB2c, dec("1700")),
        ]
    );

    // Nil-rated lines still belong in the HSN summary
    let hsn: Vec<_> = gstr1.hsn.b2c.iter().map(|r| r.hsn_sac.as_str()).collect();
    assert_eq!(hsn, ["4901", "998314"]);
    assert_eq!(gstr1.doc_issue.documents[0].series[0].total, 3);
}

#[test]
fn test_nil_json_uses_portal_keys() {
    let gstr1 = build(&[b2c_invoice(
        "INV-001",
        day(5),
        "27",
        vec![line("4901", "10", "150", "0")],
    )])
    .unwrap();
    let value = to_value(&gstr1);

    assert!(value.get("b2cs").is_none());
    assert_eq!(
        value["nil"],
        json!({ "inv": [
            { "sply_ty": "INTRAB2C", "nil_amt": 1500.0, "expt_amt": 0.0, "ngsup_amt": 0.0 }
        ] })
    );
}

#[test]
fn test_return_without_nil_rated_b2c_lines_has_no_nil_section() {
    let gstr1 = build(&[
        invoice(
            "INV-001",
            day(5),
            BUYER_SAME_STATE,
            vec![line("4901", "1", "100", "0")],
        ),
        b2c_invoice(
            "INV-002",
            day(6),
            "27",
            vec![line("998314", "1", "1000", "18")],
        ),
    ])
    .unwrap();
    assert!(gstr1.nil.is_empty());
    assert!(to_value(&gstr1).get("nil").is_none());
}

#[test]
fn test_b2cl_zero_rated_lines_go_to_table_8_not_table_5() {
    let gstr1 = build(&[
        b2c_invoice(
            "INV-001",
            day(5),
            "29",
            vec![
                line("998314", "1", "200000", "18"),
                line("4901", "10", "150", "0"),
            ],
        ),
        b2c_invoice(
            "INV-002",
            day(6),
            "29",
            vec![line("4901", "1", "150000", "0")],
        ),
        b2c_invoice("INV-003", day(7), "29", vec![line("4901", "2", "100", "0")]),
    ])
    .unwrap();

    // The mixed invoice keeps its whole value but lists only its 18% item
    assert_eq!(gstr1.b2cl.len(), 1);
    let invoices = &gstr1.b2cl[0].invoices;
    assert_eq!(invoices.len(), 1);
    assert_eq!(invoices[0].invoice_number, "INV-001");
    assert_eq!(invoices[0].invoice_value, dec("237500"));
    let rates: Vec<_> = invoices[0]
        .items
        .iter()
        .map(|i| i.detail.rate.clone())
        .collect();
    assert_eq!(rates, [dec("18")]);

    // INV-002 is all nil-rated: B2CL by value, but only in Table 8; INV-003 is B2CS
    assert!(gstr1.b2cs.is_empty());
    let nil: Vec<_> = gstr1
        .nil
        .rows
        .iter()
        .map(|r| (r.supply_type, r.nil_rated.clone()))
        .collect();
    assert_eq!(nil, [(NilSupplyType::InterB2c, dec("151700"))]);

    let hsn: Vec<_> = gstr1.hsn.b2c.iter().map(|r| r.hsn_sac.as_str()).collect();
    assert_eq!(hsn, ["4901", "998314"]);
    assert_eq!(gstr1.doc_issue.documents[0].series[0].total, 3);
}
