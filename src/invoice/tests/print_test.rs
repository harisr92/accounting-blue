use crate::invoice::print::*;
use crate::invoice::types::{GstInvoice, GstLineItem, Gstin, InvoiceError};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

fn gstin(value: &str) -> Gstin {
    Gstin::parse(value).unwrap()
}

/// `quantity` units of IT consulting at 1,500.50 each, charged at 18%
fn item(quantity: &str) -> GstLineItem {
    GstLineItem::new(
        "998314",
        "IT consulting",
        quantity.parse().unwrap(),
        "1500.50".parse().unwrap(),
        BigDecimal::from(18),
    )
    .unwrap()
}

fn invoice(buyer: &str, line_items: Vec<GstLineItem>) -> GstInvoice {
    GstInvoice::new(
        "INV/2025-26/007",
        NaiveDate::from_ymd_opt(2025, 4, 3).unwrap(),
        gstin(SELLER),
        gstin(buyer),
        line_items,
    )
    .unwrap()
}

fn parties(buyer: &str) -> InvoiceParties {
    InvoiceParties::new(
        InvoiceParty::new(
            "Acme Services",
            vec!["1 MG Road".into(), "Mumbai".into()],
            gstin(SELLER),
        ),
        InvoiceParty::new("Globex Ltd", vec!["Bengaluru".into()], gstin(buyer)),
    )
}

fn rows(count: usize) -> Vec<PrintRow> {
    let invoice = invoice(BUYER_SAME_STATE, (0..count).map(|_| item("1")).collect());
    InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE))
        .unwrap()
        .rows
}

#[test]
fn test_intra_state_invoice_prints_cgst_and_sgst() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("2"), item("1.5")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.title, "Tax Invoice");
    assert_eq!(print.invoice_number, "INV/2025-26/007");
    assert_eq!(print.invoice_date, "03-04-2025");
    assert_eq!(print.place_of_supply, "27");
    assert!(!print.is_inter_state);
    let labels: Vec<_> = print
        .tax_lines
        .iter()
        .map(|line| line.label.as_str())
        .collect();
    assert_eq!(labels, ["CGST", "SGST"]);
    assert_eq!(print.tax_lines[0].amount, "472.66");
    assert_eq!(print.tax_lines[1].amount, "472.66");
}

#[test]
fn test_inter_state_invoice_prints_igst_only() {
    let invoice = invoice(BUYER_OTHER_STATE, vec![item("2")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_OTHER_STATE)).unwrap();

    assert!(print.is_inter_state);
    assert_eq!(print.place_of_supply, "29");
    assert_eq!(print.tax_lines.len(), 1);
    assert_eq!(print.tax_lines[0].label, "IGST");
    assert_eq!(print.tax_lines[0].amount, "540.18");
}

#[test]
fn test_printed_totals_match_the_invoice_breakdown() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("2"), item("1000")]);
    let breakdown = invoice.breakdown().unwrap();
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.taxable_value, "15,03,501.00");
    assert_eq!(breakdown.taxable_value, BigDecimal::from(1_503_501));
    assert_eq!(print.total_tax, "2,70,630.18");
    assert_eq!(print.total, "17,74,131.18");
    assert_eq!(
        print.total_in_words,
        "Rupees Seventeen Lakh Seventy Four Thousand One Hundred Thirty One and Eighteen Paise Only"
    );
}

#[test]
fn test_rows_are_numbered_and_formatted() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("2"), item("1.50")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    let second = &print.rows[1];
    assert_eq!(second.serial, 2);
    assert_eq!(second.hsn_sac, "998314");
    assert_eq!(second.description, "IT consulting");
    assert_eq!(second.quantity, "1.5");
    assert_eq!(second.rate, "1,500.50");
    assert_eq!(second.taxable_value, "2,250.75");
    assert_eq!(second.gst_rate, "18%");
    assert_eq!(second.tax, "405.14");
    assert_eq!(second.amount, "2,655.89");
}

#[test]
fn test_parties_must_match_the_invoice() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);

    let result = InvoicePrint::from_invoice(&invoice, &parties(BUYER_OTHER_STATE));
    assert!(matches!(
        result,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Buyer,
            reason: PartyError::GstinMismatch { .. },
        })
    ));

    let mut unnamed = parties(BUYER_SAME_STATE);
    unnamed.seller.name = "  ".into();
    let result = InvoicePrint::from_invoice(&invoice, &unnamed);
    assert!(matches!(
        result,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Seller,
            reason: PartyError::EmptyName,
        })
    ));
}

#[test]
fn test_print_model_serialises_to_json() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    let json = serde_json::to_value(&print).unwrap();
    assert_eq!(json["total"], "1,770.60");
    assert_eq!(json["seller"]["gstin"], SELLER);
    let back: InvoicePrint = serde_json::from_value(json).unwrap();
    assert_eq!(back, print);
}

#[test]
fn test_rows_break_across_pages() {
    let capacity = RowCapacity {
        first_page: 3,
        other_pages: 5,
        totals: 2,
    };
    let sizes = |count: usize| -> Vec<usize> {
        let rows = rows(count);
        paginate_rows(&rows, capacity)
            .iter()
            .map(|page| page.len())
            .collect()
    };

    assert_eq!(sizes(1), [1]);
    assert_eq!(sizes(2), [2, 0], "totals need a page of their own");
    assert_eq!(sizes(3), [3, 0]);
    assert_eq!(sizes(6), [3, 3]);
    assert_eq!(sizes(7), [3, 4, 0]);
    assert_eq!(sizes(13), [3, 5, 5, 0]);
}

#[test]
fn test_a_page_always_exists() {
    let capacity = RowCapacity {
        first_page: 0,
        other_pages: 0,
        totals: 0,
    };
    assert_eq!(paginate_rows(&[], capacity).len(), 1);
}
