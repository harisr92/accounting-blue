//! GSTR-1 built from a month of invoices through the public API, checked on its JSON

use accounting_core::invoice::{
    GstInvoice, GstLineItem, Gstin, HsnMaster, Recipient, StateCode, SupplyKind,
};
use accounting_core::{Gstr1Error, Gstr1Return, ReturnPeriod};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::str::FromStr;

const SELLER: &str = "27AAPFU0939F1ZV";
const MAHARASHTRA_BUYER: &str = "27AAPFU0939F2ZU";
const KARNATAKA_BUYER: &str = "29AAPFU0939F1ZR";
const DELHI_BUYER: &str = "07AABCT1332L1ZG";

fn line(hsn: &str, description: &str, quantity: u32, price: &str, rate: u32) -> GstLineItem {
    GstLineItem::new(
        hsn,
        description,
        BigDecimal::from(quantity),
        BigDecimal::from_str(price).unwrap(),
        BigDecimal::from(rate),
    )
    .unwrap()
}

fn invoice(number: &str, day: u32, buyer: &str, lines: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        number,
        NaiveDate::from_ymd_opt(2024, 11, day).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(buyer).unwrap(),
        lines,
    )
    .unwrap()
}

/// Five invoices to three buyers in November 2024: intra- and inter-state, several rates and a
/// zero-rated line
fn november_invoices() -> Vec<GstInvoice> {
    vec![
        invoice(
            "INV-005",
            28,
            DELHI_BUYER,
            vec![line("4901", "Printed manuals", 10, "150", 0)],
        ),
        invoice(
            "INV-001",
            2,
            MAHARASHTRA_BUYER,
            vec![line("998314", "IT consulting", 10, "1500", 18)],
        ),
        invoice(
            "INV-002",
            9,
            KARNATAKA_BUYER,
            vec![
                line("847130", "Laptop", 2, "55000", 18),
                line("1905", "Biscuits", 100, "12.50", 5),
            ],
        ),
        invoice(
            "INV-003",
            15,
            MAHARASHTRA_BUYER,
            vec![line("998314", "IT consulting", 4, "1500", 18)],
        ),
        invoice(
            "INV-004",
            21,
            KARNATAKA_BUYER,
            vec![line("8471", "Desktop", 1, "40000", 18)],
        ),
    ]
}

fn gstr1_json() -> Value {
    let seller = Gstin::parse(SELLER).unwrap();
    let period = ReturnPeriod::new(2024, 11).unwrap();
    let gstr1 =
        Gstr1Return::build(&seller, period, &november_invoices(), HsnMaster::global()).unwrap();
    serde_json::from_str(&gstr1.to_json().unwrap()).unwrap()
}

fn sum(values: impl Iterator<Item = f64>) -> f64 {
    (values.sum::<f64>() * 100.0).round() / 100.0
}

#[test]
fn test_five_invoices_become_one_gstr1_return() {
    let value = gstr1_json();
    assert_eq!(value["gstin"], json!(SELLER));
    assert_eq!(value["fp"], json!("112024"));

    let buyers: Vec<_> = value["b2b"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["ctin"].clone())
        .collect();
    assert_eq!(
        buyers,
        [
            json!(DELHI_BUYER),
            json!(MAHARASHTRA_BUYER),
            json!(KARNATAKA_BUYER)
        ]
    );

    let invoice_numbers: Vec<_> = value["b2b"][1]["inv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inv| inv["inum"].clone())
        .collect();
    assert_eq!(invoice_numbers, [json!("INV-001"), json!("INV-003")]);
}

#[test]
fn test_invoice_values_and_tax_columns() {
    let value = gstr1_json();

    // Intra-state: 15000 at 18% as CGST + SGST
    let maharashtra = &value["b2b"][1]["inv"][0];
    assert_eq!(maharashtra["val"], json!(17700.0));
    assert_eq!(maharashtra["pos"], json!("27"));
    assert_eq!(maharashtra["itms"][0]["itm_det"]["camt"], json!(1350.0));
    assert_eq!(maharashtra["itms"][0]["itm_det"]["samt"], json!(1350.0));

    // Inter-state with two rates: 1250 at 5% and 110000 at 18%, as IGST
    let karnataka = &value["b2b"][2]["inv"][0];
    assert_eq!(karnataka["val"], json!(131112.5));
    assert_eq!(karnataka["pos"], json!("29"));
    let items = karnataka["itms"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["itm_det"]["rt"], json!(5.0));
    assert_eq!(items[0]["itm_det"]["iamt"], json!(62.5));
    assert_eq!(items[1]["itm_det"]["rt"], json!(18.0));
    assert_eq!(items[1]["itm_det"]["iamt"], json!(19800.0));

    // Zero-rated
    let delhi = &value["b2b"][0]["inv"][0];
    assert_eq!(delhi["val"], json!(1500.0));
    assert_eq!(delhi["itms"][0]["itm_det"]["rt"], json!(0.0));
    assert_eq!(delhi["itms"][0]["itm_det"]["iamt"], json!(0.0));
}

#[test]
fn test_hsn_summary_and_documents_agree_with_the_invoices() {
    let value = gstr1_json();
    let invoices: Vec<&Value> = value["b2b"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|party| party["inv"].as_array().unwrap())
        .collect();
    let rows = value["hsn"]["hsn_b2b"].as_array().unwrap();

    // 998314@18, 1905@5, 4901@0, 8471@18, 847130@18
    assert_eq!(rows.len(), 5);
    let invoice_total = sum(invoices.iter().map(|inv| inv["val"].as_f64().unwrap()));
    let hsn_total = sum(rows.iter().map(|row| row["val"].as_f64().unwrap()));
    assert_eq!(invoice_total, 204_592.5);
    assert_eq!(hsn_total, invoice_total);

    let consulting = rows
        .iter()
        .find(|row| row["hsn_sc"] == json!("998314"))
        .unwrap();
    // Services report NA with no quantity; goods keep their unit count
    assert_eq!(consulting["uqc"], json!("NA"));
    assert_eq!(consulting["qty"], json!(0.0));
    assert_eq!(consulting["txval"], json!(21000.0));
    let biscuits = rows
        .iter()
        .find(|row| row["hsn_sc"] == json!("1905"))
        .unwrap();
    assert_eq!(biscuits["uqc"], json!("OTH"));
    assert_eq!(biscuits["qty"], json!(100.0));

    assert_eq!(
        value["doc_issue"]["doc_det"][0]["docs"][0],
        json!({ "num": 1, "from": "INV-001", "to": "INV-005", "totnum": 5, "cancel": 0, "net_issue": 5 })
    );
}

#[test]
fn test_a_bad_invoice_fails_the_whole_return() {
    let seller = Gstin::parse(SELLER).unwrap();
    let october = ReturnPeriod::new(2024, 10).unwrap();
    let result = Gstr1Return::build(&seller, october, &november_invoices(), HsnMaster::global());
    assert!(matches!(result, Err(Gstr1Error::OutsidePeriod { .. })));

    let error: accounting_core::Error = result.unwrap_err().into();
    assert!(matches!(error, accounting_core::Error::Gstr1(_)));
}

fn retail_invoice(number: &str, day: u32, state: &str, lines: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        number,
        NaiveDate::from_ymd_opt(2024, 11, day).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Recipient::unregistered(StateCode::parse(state).unwrap()),
        lines,
    )
    .unwrap()
}

#[test]
fn test_large_inter_state_sales_to_unregistered_buyers_are_reported_as_b2cl() {
    let mut invoices = november_invoices();
    let laptops = retail_invoice(
        "INV-006",
        29,
        "29",
        vec![line("847130", "Laptop", 2, "55000", 18)],
    );
    assert_eq!(laptops.supply_kind().unwrap(), SupplyKind::B2cl);
    invoices.push(laptops);

    let seller = Gstin::parse(SELLER).unwrap();
    let period = ReturnPeriod::new(2024, 11).unwrap();
    let gstr1 = Gstr1Return::build(&seller, period, &invoices, HsnMaster::global()).unwrap();
    let value: Value = serde_json::from_str(&gstr1.to_json().unwrap()).unwrap();

    assert_eq!(value["b2b"].as_array().unwrap().len(), 3);
    assert_eq!(value["b2cl"][0]["pos"], json!("29"));
    assert_eq!(value["b2cl"][0]["inv"][0]["val"], json!(129800.0));
    assert_eq!(
        value["b2cl"][0]["inv"][0]["itms"][0]["itm_det"]["iamt"],
        json!(19800.0)
    );
    assert_eq!(value["hsn"]["hsn_b2b"].as_array().unwrap().len(), 5);
    assert_eq!(value["hsn"]["hsn_b2c"][0]["hsn_sc"], json!("847130"));
    assert_eq!(
        value["doc_issue"]["doc_det"][0]["docs"][0]["to"],
        json!("INV-006")
    );
    assert_eq!(
        value["doc_issue"]["doc_det"][0]["docs"][0]["totnum"],
        json!(6)
    );
}

#[test]
fn test_small_sales_to_unregistered_buyers_are_reported_as_b2cs() {
    let mut invoices = november_invoices();
    // Inter-state but small, and intra-state of any value: both B2CS
    invoices.push(retail_invoice(
        "INV-006",
        29,
        "29",
        vec![line("1905", "Biscuits", 10, "12.50", 5)],
    ));
    invoices.push(retail_invoice(
        "INV-007",
        30,
        "27",
        vec![line("847130", "Laptop", 3, "55000", 18)],
    ));
    for invoice in &invoices[5..] {
        assert_eq!(invoice.supply_kind().unwrap(), SupplyKind::B2cs);
    }

    let seller = Gstin::parse(SELLER).unwrap();
    let period = ReturnPeriod::new(2024, 11).unwrap();
    let gstr1 = Gstr1Return::build(&seller, period, &invoices, HsnMaster::global()).unwrap();
    let value: Value = serde_json::from_str(&gstr1.to_json().unwrap()).unwrap();

    assert_eq!(value["b2b"].as_array().unwrap().len(), 3);
    assert!(value.get("b2cl").is_none());
    assert_eq!(
        value["b2cs"],
        json!([
            {
                "sply_ty": "INTRA", "rt": 18.0, "typ": "OE", "pos": "27",
                "txval": 165000.0, "camt": 14850.0, "samt": 14850.0, "csamt": 0.0
            },
            {
                "sply_ty": "INTER", "rt": 5.0, "typ": "OE", "pos": "29",
                "txval": 125.0, "iamt": 6.25, "csamt": 0.0
            }
        ])
    );
    assert_eq!(value["hsn"]["hsn_b2b"].as_array().unwrap().len(), 5);
    assert_eq!(value["hsn"]["hsn_b2c"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["doc_issue"]["doc_det"][0]["docs"][0]["to"],
        json!("INV-007")
    );
    assert_eq!(
        value["doc_issue"]["doc_det"][0]["docs"][0]["totnum"],
        json!(7)
    );
}
