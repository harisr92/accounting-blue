use crate::document::discount::Discount;
use crate::gst::invoice::payment::PaymentDetails;
use crate::gst::invoice::print::*;
use crate::gst::invoice::seller::{
    CompositionRate, SellerRegistration, COMPOSITION_DECLARATION, UNREGISTERED_DECLARATION,
};
use crate::gst::invoice::types::{
    GstInvoice, GstLineItem, Gstin, InvoiceError, Recipient, StateCode, SupplyTreatment,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

fn gstin(value: &str) -> Gstin {
    Gstin::parse(value).unwrap()
}

/// `quantity` units of printed books at 150 each, taxable at `rate`
fn item_at(quantity: &str, rate: u32) -> GstLineItem {
    GstLineItem::new(
        "4901",
        "Printed books",
        quantity.parse().unwrap(),
        "150".parse().unwrap(),
        BigDecimal::from(rate),
    )
    .unwrap()
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
    assert_eq!(print.place_of_supply, "27 - Maharashtra");
    assert_eq!(print.buyer_state, "27 - Maharashtra");
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
    assert_eq!(print.place_of_supply, "29 - Karnataka");
    assert_eq!(print.buyer_state, "29 - Karnataka");
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

fn unregistered_invoice(place_of_supply: &str) -> GstInvoice {
    GstInvoice::new(
        "INV/2025-26/008",
        NaiveDate::from_ymd_opt(2025, 4, 3).unwrap(),
        gstin(SELLER),
        Recipient::unregistered(StateCode::parse(place_of_supply).unwrap()),
        vec![item("100")],
    )
    .unwrap()
}

fn unregistered_parties() -> InvoiceParties {
    InvoiceParties::new(
        parties(BUYER_SAME_STATE).seller,
        InvoiceParty::unregistered("Ravi Kumar", vec!["Bengaluru".into()]),
    )
}

#[test]
fn test_unregistered_buyer_prints_without_a_gstin() {
    let print =
        InvoicePrint::from_invoice(&unregistered_invoice("29"), &unregistered_parties()).unwrap();

    assert_eq!(print.title, "Tax Invoice");
    assert_eq!(print.place_of_supply, "29 - Karnataka");
    assert_eq!(print.buyer_state, "29 - Karnataka");
    assert!(print.is_inter_state);
    assert_eq!(print.buyer.gstin, None);
    assert_eq!(print.tax_lines[0].label, "IGST");

    let json = serde_json::to_value(&print).unwrap();
    assert!(json["buyer"].get("gstin").is_none());
    let back: InvoicePrint = serde_json::from_value(json).unwrap();
    assert_eq!(back, print);
}

#[test]
fn test_buyer_gstin_must_agree_with_the_recipient() {
    let to_unregistered =
        InvoicePrint::from_invoice(&unregistered_invoice("29"), &parties(BUYER_OTHER_STATE));
    assert!(matches!(
        to_unregistered,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Buyer,
            reason: PartyError::UnexpectedGstin { .. },
        })
    ));

    let to_registered = InvoicePrint::from_invoice(
        &invoice(BUYER_OTHER_STATE, vec![item("1")]),
        &unregistered_parties(),
    );
    assert!(matches!(
        to_registered,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Buyer,
            reason: PartyError::MissingGstin { .. },
        })
    ));
}

/// An invoice to an unregistered buyer in Maharashtra for one line of `taxable` rupees at 18%
fn walk_in_invoice(taxable: &str) -> GstInvoice {
    let line = GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(1),
        taxable.parse().unwrap(),
        BigDecimal::from(18),
    )
    .unwrap();
    GstInvoice::new(
        "INV/2025-26/009",
        NaiveDate::from_ymd_opt(2025, 4, 3).unwrap(),
        gstin(SELLER),
        Recipient::unregistered(StateCode::parse("27").unwrap()),
        vec![line],
    )
    .unwrap()
}

fn with_buyer(buyer: InvoiceParty) -> InvoiceParties {
    InvoiceParties::new(parties(BUYER_SAME_STATE).seller, buyer)
}

fn buyer_error(invoice: &GstInvoice, buyer: InvoiceParty) -> Option<PartyError> {
    match InvoicePrint::from_invoice(invoice, &with_buyer(buyer)) {
        Ok(_) => None,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Buyer,
            reason,
        }) => Some(reason),
        Err(other) => panic!("unexpected error {other}"),
    }
}

#[test]
fn test_walk_in_buyer_needs_no_details_below_rule_46_threshold() {
    let invoice = walk_in_invoice("49999.99");
    let print = InvoicePrint::from_invoice(&invoice, &with_buyer(InvoiceParty::walk_in())).unwrap();

    assert!(!print.buyer.has_name());
    assert!(print.buyer.address.is_empty());
    assert_eq!(print.buyer.gstin, None);
    assert_eq!(print.place_of_supply, "27 - Maharashtra");
    // Tax takes the invoice value past 50,000; Rule 46 looks at the taxable value
    assert_eq!(print.total, "58,999.99");
}

#[test]
fn test_unregistered_buyer_details_are_required_from_rule_46_threshold() {
    let invoice = walk_in_invoice("50000");
    let named = |address: Vec<String>| InvoiceParty::unregistered("Ravi Kumar", address);

    assert_eq!(
        buyer_error(&invoice, InvoiceParty::walk_in()),
        Some(PartyError::EmptyName)
    );
    assert_eq!(
        buyer_error(&invoice, named(Vec::new())),
        Some(PartyError::MissingAddress)
    );
    assert_eq!(
        buyer_error(&invoice, named(vec!["  ".into()])),
        Some(PartyError::MissingAddress)
    );
    assert_eq!(buyer_error(&invoice, named(vec!["Pune".into()])), None);
    // Below the threshold, check_against takes a walk-in buyer too
    assert!(with_buyer(InvoiceParty::walk_in())
        .check_against(&walk_in_invoice("100"))
        .is_ok());
}

#[test]
fn test_registered_buyer_and_seller_still_need_a_name() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let mut unnamed = parties(BUYER_SAME_STATE);
    unnamed.buyer.name = String::new();
    assert!(matches!(
        InvoicePrint::from_invoice(&invoice, &unnamed),
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Buyer,
            reason: PartyError::EmptyName,
        })
    ));

    let mut small = with_buyer(InvoiceParty::walk_in());
    small.seller.name = " ".into();
    assert!(matches!(
        InvoicePrint::from_invoice(&walk_in_invoice("100"), &small),
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Seller,
            reason: PartyError::EmptyName,
        })
    ));
}

fn book(treatment: SupplyTreatment) -> GstLineItem {
    let build = match treatment {
        SupplyTreatment::Exempt => GstLineItem::exempt,
        _ => GstLineItem::non_gst,
    };
    build(
        "4901",
        "Printed books",
        "2".parse().unwrap(),
        "150".parse().unwrap(),
    )
    .unwrap()
}

#[test]
fn test_invoice_of_only_exempt_and_non_gst_lines_is_a_bill_of_supply() {
    let lines = vec![book(SupplyTreatment::Exempt), book(SupplyTreatment::NonGst)];
    let invoice = invoice(BUYER_SAME_STATE, lines);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.title, BILL_OF_SUPPLY_TITLE);
    let rates: Vec<_> = print.rows.iter().map(|r| r.gst_rate.as_str()).collect();
    assert_eq!(rates, [EXEMPT_RATE_LABEL, NON_GST_RATE_LABEL]);
    assert!(print.tax_lines.is_empty());
}

#[test]
fn test_nil_rated_invoice_stays_a_tax_invoice() {
    let print = |lines| {
        let invoice = invoice(BUYER_SAME_STATE, lines);
        InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap()
    };

    let nil_only = print(vec![item_at("1", 0)]);
    assert_eq!(nil_only.title, TAX_INVOICE_TITLE);
    assert_eq!(nil_only.rows[0].gst_rate, "0%");
    assert!(nil_only.tax_lines.is_empty());

    let nil_and_exempt = print(vec![item_at("1", 0), book(SupplyTreatment::Exempt)]);
    assert_eq!(nil_and_exempt.title, TAX_INVOICE_TITLE);
}

#[test]
fn test_invoice_with_any_taxed_line_stays_a_tax_invoice() {
    let invoice = invoice(
        BUYER_SAME_STATE,
        vec![book(SupplyTreatment::Exempt), item("1")],
    );
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.title, TAX_INVOICE_TITLE);
    let rates: Vec<_> = print.rows.iter().map(|r| r.gst_rate.as_str()).collect();
    assert_eq!(rates, ["Exempt", "18%"]);
}

/// Consulting at 10% off with books at full price, and 10% off the subtotal
fn discounted_invoice() -> GstInvoice {
    let consulting = item("2")
        .with_discount(Discount::Percent(BigDecimal::from(10)))
        .unwrap();
    invoice(BUYER_SAME_STATE, vec![consulting, item_at("4", 5)])
        .with_discount(Discount::Percent(BigDecimal::from(10)))
        .unwrap()
}

#[test]
fn test_discounts_are_printed_between_subtotal_and_taxable_value() {
    let print =
        InvoicePrint::from_invoice(&discounted_invoice(), &parties(BUYER_SAME_STATE)).unwrap();

    // Each row is taxed on its value after its own discount and its share of the invoice
    // discount: 2,700.90 - 270.09 and 600.00 - 60.00
    assert_eq!(print.rows[0].discount.as_deref(), Some("300.10"));
    assert_eq!(print.rows[0].taxable_value, "2,430.81");
    assert_eq!(print.rows[1].discount, None);
    assert_eq!(print.rows[1].taxable_value, "540.00");
    assert_eq!(print.rows[1].tax, "27.00");
    assert_eq!(print.rows[1].amount, "567.00");
    assert_eq!(print.subtotal, "3,300.90");
    assert_eq!(print.discount.as_deref(), Some("330.09"));
    assert_eq!(print.discount_label.as_deref(), Some("Discount (10%)"));
    assert_eq!(print.taxable_value, "2,970.81");
    assert!(print.has_line_discounts());
    assert!(print.shows_subtotal());
}

#[test]
fn test_flat_invoice_discount_is_labelled_without_a_rate() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("2")])
        .with_discount(Discount::Amount(BigDecimal::from(1)))
        .unwrap();
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.discount_label.as_deref(), Some(DISCOUNT_LABEL));
    assert_eq!(print.taxable_value, "3,000.00");
    assert!(!print.has_line_discounts());
    assert!(print.shows_subtotal());
}

#[test]
fn test_invoice_without_discounts_prints_no_discount() {
    let print = InvoicePrint::from_invoice(
        &invoice(BUYER_SAME_STATE, vec![item("2")]),
        &parties(BUYER_SAME_STATE),
    )
    .unwrap();

    assert_eq!(print.discount, None);
    assert_eq!(print.discount_label, None);
    assert_eq!(print.subtotal, print.taxable_value);
    assert!(!print.shows_subtotal());
    let json = serde_json::to_string(&print).unwrap();
    assert!(!json.contains("discount"));
}

#[test]
fn test_zero_discounts_are_not_printed() {
    let line = item("2")
        .with_discount(Discount::Amount(BigDecimal::from(0)))
        .unwrap();
    let invoice = invoice(BUYER_SAME_STATE, vec![line])
        .with_discount(Discount::Percent(BigDecimal::from(0)))
        .unwrap();
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.rows[0].discount, None);
    assert_eq!(print.discount, None);
    assert_eq!(print.discount_label, None);
    assert!(!print.has_line_discounts());
    assert!(!print.shows_subtotal());
}

#[test]
fn test_quantity_prints_with_its_unit_when_there_is_one() {
    let invoice = invoice(
        BUYER_SAME_STATE,
        vec![item_at("5", 0).with_unit("BKT"), item_at("2.5", 0)],
    );
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.rows[0].quantity, "5 BKT");
    assert_eq!(print.rows[1].quantity, "2.5");
}

#[test]
fn test_invoice_prints_no_details_until_they_are_given() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();

    assert_eq!(print.due_date, None);
    assert_eq!(print.reference, None);
    assert_eq!(print.ship_to, None);
    assert!(print.notes.is_empty());
    let json = serde_json::to_value(&print).unwrap();
    for field in ["due_date", "reference", "ship_to", "notes"] {
        assert!(json.get(field).is_none(), "{field} serialised: {json}");
    }
}

#[test]
fn test_details_print_the_due_date_reference_delivery_address_and_notes() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let ship_to = InvoiceParty::unregistered("Site office", vec!["Plot 4, MIDC".into()]);
    let details = InvoiceDetails {
        due_date: NaiveDate::from_ymd_opt(2025, 4, 18),
        reference: Some(" PO-7781 ".into()),
        ship_to: Some(ship_to.clone()),
        notes: vec!["Thank you.".into(), "  ".into()],
    };
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE))
        .unwrap()
        .with_details(&details);

    assert_eq!(print.due_date.as_deref(), Some("18-04-2025"));
    assert_eq!(print.reference.as_deref(), Some("PO-7781"));
    assert_eq!(print.ship_to, Some(ship_to));
    assert_eq!(print.notes, ["Thank you."]);
}

#[test]
fn test_blank_reference_is_left_out() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let details = InvoiceDetails {
        reference: Some("   ".into()),
        ..InvoiceDetails::default()
    };
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE))
        .unwrap()
        .with_details(&details);
    assert_eq!(print.reference, None);
}

fn upi_payment() -> PaymentDetails {
    PaymentDetails {
        bank_name: Some("HDFC Bank".into()),
        account_number: Some("50100012345678".into()),
        ifsc: Some("HDFC0001234".into()),
        upi_id: Some("acme@okhdfc".into()),
        ..PaymentDetails::default()
    }
}

#[test]
fn test_invoice_prints_no_payment_details_until_they_are_given() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();
    assert_eq!(print.payment, None);
    assert_eq!(print.upi_uri, None);
    assert_eq!(print.total_amount.to_string(), "1770.60");

    let empty = print.with_payment(PaymentDetails::default());
    assert_eq!(empty.payment, None);
    assert_eq!(empty.upi_uri, None);
}

#[test]
fn test_payment_details_default_the_payee_and_ask_upi_for_the_total() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE))
        .unwrap()
        .with_payment(upi_payment());

    let payment = print.payment.as_ref().unwrap();
    assert_eq!(
        payment.payee_name.as_deref(),
        Some(print.seller.name.as_str())
    );
    assert_eq!(payment.ifsc.as_deref(), Some("HDFC0001234"));
    let uri = print.upi_uri.as_deref().unwrap();
    assert!(uri.starts_with("upi://pay?pa=acme%40okhdfc&pn="), "{uri}");
    assert!(
        uri.contains("&am=1770.60&cu=INR&tn=INV%2F2025-26%2F007"),
        "{uri}"
    );
}

#[test]
fn test_upi_link_asks_for_the_amount_due_when_given() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let base = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();
    let part_paid = base.clone().with_payment(PaymentDetails {
        amount_due: Some("500".parse().unwrap()),
        ..upi_payment()
    });
    assert!(part_paid.upi_uri.unwrap().contains("&am=500.00&"));

    let paid = base.with_payment(PaymentDetails {
        amount_due: Some(BigDecimal::from(0)),
        ..upi_payment()
    });
    assert_eq!(paid.upi_uri, None);
    assert!(paid.payment.is_some(), "bank details still print");
}

#[test]
fn test_no_upi_link_without_a_upi_id() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE))
        .unwrap()
        .with_payment(PaymentDetails {
            upi_id: Some(" ".into()),
            ..upi_payment()
        });
    assert!(print.payment.is_some());
    assert_eq!(print.upi_uri, None);
}

#[test]
fn test_print_charges_tax_and_has_no_declaration_by_default() {
    let taxed = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let exempt = invoice(BUYER_SAME_STATE, vec![book(SupplyTreatment::Exempt)]);
    for invoice in [taxed, exempt] {
        let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();
        assert!(print.charges_tax);
        assert_eq!(print.declaration, None);
        assert_eq!(print.seller_state, "27 - Maharashtra");
    }
}

#[test]
fn test_print_json_without_new_fields_reads_as_charging_tax() {
    let invoice = invoice(BUYER_SAME_STATE, vec![item("1")]);
    let print = InvoicePrint::from_invoice(&invoice, &parties(BUYER_SAME_STATE)).unwrap();
    let mut json = serde_json::to_value(&print).unwrap();
    let fields = json.as_object_mut().unwrap();
    assert!(!fields.contains_key("declaration"));
    fields.remove("charges_tax");
    fields.remove("seller_state");

    let back: InvoicePrint = serde_json::from_value(json).unwrap();
    assert!(back.charges_tax);
    assert_eq!(back.declaration, None);
    assert_eq!(back.seller_state, "");
}

fn composition_seller() -> SellerRegistration {
    SellerRegistration::Composition {
        gstin: gstin(SELLER),
        rate: CompositionRate::TraderOrManufacturer,
    }
}

fn unregistered_seller() -> SellerRegistration {
    SellerRegistration::Unregistered {
        state: StateCode::parse("29").unwrap(),
    }
}

fn bill_of_supply(seller: SellerRegistration) -> GstInvoice {
    GstInvoice::new(
        "BOS/2025-26/001",
        NaiveDate::from_ymd_opt(2025, 4, 3).unwrap(),
        seller,
        Recipient::unregistered(StateCode::parse("29").unwrap()),
        vec![item_at("2", 0)],
    )
    .unwrap()
}

fn seller_parties(seller: InvoiceParty) -> InvoiceParties {
    InvoiceParties::new(seller, InvoiceParty::walk_in())
}

#[test]
fn test_composition_bill_of_supply_prints_its_declaration_without_tax() {
    let parties = seller_parties(parties(BUYER_SAME_STATE).seller);
    let print =
        InvoicePrint::from_invoice(&bill_of_supply(composition_seller()), &parties).unwrap();

    assert_eq!(print.title, BILL_OF_SUPPLY_TITLE);
    assert!(!print.charges_tax);
    assert_eq!(print.declaration.as_deref(), Some(COMPOSITION_DECLARATION));
    assert_eq!(print.seller_state, "27 - Maharashtra");
}

#[test]
fn test_unregistered_seller_prints_a_bill_of_supply_without_a_gstin() {
    let seller = InvoiceParty::unregistered("Ravi Stores", vec!["Bengaluru".into()]);
    let print = InvoicePrint::from_invoice(
        &bill_of_supply(unregistered_seller()),
        &seller_parties(seller),
    )
    .unwrap();

    assert_eq!(print.title, BILL_OF_SUPPLY_TITLE);
    assert!(!print.charges_tax);
    assert_eq!(print.declaration.as_deref(), Some(UNREGISTERED_DECLARATION));
    assert_eq!(print.seller_state, "29 - Karnataka");
    assert_eq!(print.total, "300.00");
}

#[test]
fn test_unregistered_seller_party_with_a_gstin_is_refused() {
    let result = InvoicePrint::from_invoice(
        &bill_of_supply(unregistered_seller()),
        &seller_parties(parties(BUYER_SAME_STATE).seller),
    );

    assert!(matches!(
        result,
        Err(InvoiceError::InvalidParty {
            role: PartyRole::Seller,
            reason: PartyError::UnexpectedGstin { .. },
        })
    ));
}

#[test]
fn test_bill_of_supply_with_a_taxed_line_is_not_printed() {
    let mut bill = bill_of_supply(composition_seller());
    bill.line_items.push(item_at("1", 5));
    let parties = seller_parties(parties(BUYER_SAME_STATE).seller);

    assert!(matches!(
        InvoicePrint::from_invoice(&bill, &parties),
        Err(InvoiceError::TaxOnBillOfSupply { line: 1 })
    ));
}
