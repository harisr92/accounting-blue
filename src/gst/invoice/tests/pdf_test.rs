use crate::document::discount::Discount;
use crate::gst::invoice::payment::PaymentDetails;
use crate::gst::invoice::pdf::*;
use crate::gst::invoice::print::{InvoiceDetails, InvoiceParties, InvoiceParty, InvoicePrint};
use crate::gst::invoice::seller::{SellerRegistration, UNREGISTERED_DECLARATION};
use crate::gst::invoice::types::{
    GstInvoice, GstLineItem, Gstin, InvoiceError, Recipient, StateCode,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use printpdf::{BuiltinFont, PdfDocument, PdfParseOptions};

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER: &str = "29AAPFU0939F1ZR";
/// A 2x1 pixel RGB PNG
const LOGO_PNG: [u8; 70] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x7B, 0x40, 0xE8,
    0xDD, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x38, 0x21, 0x07, 0x04,
    0x27, 0x00, 0x07, 0x23, 0x02, 0x09, 0xE4, 0x58, 0x88, 0x39, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn gstin(value: &str) -> Gstin {
    Gstin::parse(value).unwrap()
}

fn item(description: &str) -> GstLineItem {
    GstLineItem::new(
        "998314",
        description,
        BigDecimal::from(3),
        "1250.75".parse().unwrap(),
        BigDecimal::from(18),
    )
    .unwrap()
}

fn invoice(lines: usize) -> GstInvoice {
    GstInvoice::new(
        "INV/2025-26/042",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        (1..=lines)
            .map(|n| item(&format!("Consulting, phase {n}")))
            .collect(),
    )
    .unwrap()
}

fn parties() -> InvoiceParties {
    InvoiceParties::new(
        InvoiceParty::new(
            "Acme Services Pvt Ltd",
            vec!["4th Floor, Nariman Point".into(), "Mumbai 400021".into()],
            gstin(SELLER),
        ),
        InvoiceParty::new("Globex Ltd", vec!["Bengaluru 560001".into()], gstin(BUYER)),
    )
}

/// The text of each page of a rendered PDF
fn pages(pdf: &[u8]) -> Vec<String> {
    let document = PdfDocument::parse(pdf, &PdfParseOptions::default(), &mut Vec::new()).unwrap();
    document
        .extract_text()
        .into_iter()
        .map(|page| page.join("\n"))
        .collect()
}

#[test]
fn test_invoice_renders_as_a_one_page_pdf() {
    let options = PdfOptions {
        terms: vec!["Payment due within 30 days.".into()],
        ..PdfOptions::default()
    };
    let pdf = invoice(2).to_pdf(&parties(), &options).unwrap();

    assert!(pdf.starts_with(b"%PDF-"));
    let pages = pages(&pdf);
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    for expected in [
        "Tax Invoice",
        "INV/2025-26/042",
        "30-06-2025",
        "Acme Services Pvt Ltd",
        "Globex Ltd",
        "GSTIN: 29AAPFU0939F1ZR",
        "Place of Supply: 29 - Karnataka",
        "State: 29 - Karnataka",
        "Consulting, phase 2",
        "IGST",
        "Rs. 8,855.32",
        "Rupees Eight Thousand Eight Hundred Fifty Five and Thirty Two Paise Only",
        "Payment due within 30 days.",
        "Authorised Signatory",
        "Page 1 of 1",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
}

#[test]
fn test_long_invoices_continue_on_later_pages() {
    let pdf = invoice(60)
        .to_pdf(&parties(), &PdfOptions::default())
        .unwrap();
    let pages = pages(&pdf);

    assert!(pages.len() >= 2, "60 lines should not fit on one page");
    let count = pages.len();
    assert!(pages[count - 1].contains(&format!("Page {count} of {count}")));
    assert!(pages[1].contains("Tax Invoice (continued)"));
    assert!(pages
        .iter()
        .any(|page| page.contains("Consulting, phase 60")));
    assert!(pages[count - 1].contains("Amount in words"));
    assert!(!pages[0].contains("Amount in words"));
}

#[test]
fn test_every_line_is_printed_exactly_once() {
    let pdf = invoice(45)
        .to_pdf(&parties(), &PdfOptions::default())
        .unwrap();
    let text = pages(&pdf).join("\n");

    for n in 1..=45 {
        let description = format!("Consulting, phase {n}");
        let found = text
            .lines()
            .filter(|line| line.trim() == description)
            .count();
        assert_eq!(found, 1, "{description}");
    }
}

#[test]
fn test_custom_font_is_embedded() {
    let helvetica = BuiltinFont::Helvetica.get_subset_font().bytes;
    let options = PdfOptions {
        font: PdfFont::Custom {
            regular: helvetica,
            bold: None,
        },
        ..PdfOptions::default()
    };
    let pdf = invoice(1).to_pdf(&parties(), &options).unwrap();

    let document = PdfDocument::parse(&pdf, &PdfParseOptions::default(), &mut Vec::new()).unwrap();
    assert_eq!(document.pages.len(), 1);
    assert_eq!(
        document.resources.fonts.map.len(),
        1,
        "the custom font is embedded"
    );
    let builtin = invoice(1)
        .to_pdf(&parties(), &PdfOptions::default())
        .unwrap();
    assert!(pdf.len() > builtin.len());
}

#[test]
fn test_bad_font_bytes_are_rejected() {
    let options = PdfOptions {
        font: PdfFont::Custom {
            regular: b"not a font".to_vec(),
            bold: None,
        },
        ..PdfOptions::default()
    };
    let result = invoice(1).to_pdf(&parties(), &options);

    assert!(matches!(
        result,
        Err(PdfError::InvalidFont {
            face: FontFace::Regular
        })
    ));
}

#[test]
fn test_text_the_font_cannot_draw_is_refused() {
    let options = PdfOptions {
        currency_label: "₹".into(),
        ..PdfOptions::default()
    };
    let result = invoice(1).to_pdf(&parties(), &options);

    assert!(matches!(result, Err(PdfError::MissingGlyph { ch: '₹' })));
}

#[test]
fn test_parties_that_do_not_match_are_refused() {
    let mut parties = parties();
    parties.buyer.gstin = Some(gstin(SELLER));
    let result = invoice(1).to_pdf(&parties, &PdfOptions::default());

    assert!(matches!(
        result,
        Err(PdfError::Invoice(InvoiceError::InvalidParty { .. }))
    ));
}

#[test]
fn test_large_amounts_are_printed_in_full() {
    let line = GstLineItem::new(
        "8471",
        "Server cluster",
        BigDecimal::from(1_000_000),
        BigDecimal::from(10_000_000),
        BigDecimal::from(18),
    )
    .unwrap();
    let invoice = GstInvoice::new(
        "INV/2025-26/043",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![line],
    )
    .unwrap();
    let pdf = invoice.to_pdf(&parties(), &PdfOptions::default()).unwrap();
    let page = pages(&pdf).join("\n");

    for expected in ["1000000", "1,00,00,000.00", "18,00,00,00,00,000.00"] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
}

#[test]
fn test_labels_the_font_cannot_draw_are_refused() {
    // Symbol has digits and punctuation but no Latin letters, so every caller-supplied string
    // below can be drawn and only the renderer's own labels ("Tax Invoice", "BILL TO", ...) can't
    let symbol = BuiltinFont::Symbol.get_subset_font().bytes;
    let invoice = GstInvoice::new(
        "2025/042",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![GstLineItem::new(
            "998314",
            "1",
            BigDecimal::from(1),
            BigDecimal::from(1),
            BigDecimal::from(18),
        )
        .unwrap()],
    )
    .unwrap();
    let parties = InvoiceParties::new(
        InvoiceParty::new("1", Vec::new(), gstin(SELLER)),
        InvoiceParty::new("2", Vec::new(), gstin(BUYER)),
    );
    let options = PdfOptions {
        font: PdfFont::Custom {
            regular: symbol,
            bold: None,
        },
        currency_label: String::new(),
        ..PdfOptions::default()
    };
    let result = invoice.to_pdf(&parties, &options);

    assert!(
        matches!(result, Err(PdfError::MissingGlyph { ch }) if ch.is_ascii_alphabetic()),
        "{result:?}"
    );
}

#[test]
fn test_terms_too_long_for_a_page_are_refused() {
    let options = PdfOptions {
        terms: (1..=80).map(|n| format!("Term number {n}.")).collect(),
        ..PdfOptions::default()
    };
    let result = invoice(1).to_pdf(&parties(), &options);

    assert!(
        matches!(result, Err(PdfError::TermsTooLong { lines: 80, max_lines }) if max_lines < 80),
        "{result:?}"
    );
}

#[test]
fn test_terms_that_just_fit_share_a_page_with_the_totals() {
    let max_lines = match invoice(1).to_pdf(
        &parties(),
        &PdfOptions {
            terms: vec!["Term.".into(); 200],
            ..PdfOptions::default()
        },
    ) {
        Err(PdfError::TermsTooLong { max_lines, .. }) => max_lines,
        other => panic!("{other:?}"),
    };
    let options = PdfOptions {
        terms: (1..=max_lines)
            .map(|n| format!("Term number {n}."))
            .collect(),
        ..PdfOptions::default()
    };
    let pages = pages(&invoice(40).to_pdf(&parties(), &options).unwrap());

    let last = pages.last().unwrap();
    assert!(last.contains("Amount in words"));
    assert!(last.contains(&format!("Term number {max_lines}.")));
    assert!(last.contains("Authorised Signatory"));
}

#[test]
fn test_unregistered_buyer_renders_without_a_gstin() {
    let invoice = GstInvoice::new(
        "INV/2025-26/043",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        Recipient::unregistered(StateCode::parse("29").unwrap()),
        vec![item("Consulting")],
    )
    .unwrap();
    let parties = InvoiceParties::new(
        parties().seller,
        InvoiceParty::unregistered("Ravi Kumar", vec!["Bengaluru 560001".into()]),
    );
    let pdf = invoice.to_pdf(&parties, &PdfOptions::default()).unwrap();

    let page = &pages(&pdf)[0];
    for expected in [
        "Ravi Kumar",
        "GSTIN: Unregistered",
        "Place of Supply: 29 - Karnataka",
        "State: 29 - Karnataka",
        "IGST",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
    assert!(!page.contains(BUYER), "no buyer GSTIN expected in:\n{page}");
}

#[test]
fn test_no_footer_note_is_printed_by_default() {
    let page = &pages(
        &invoice(1)
            .to_pdf(&parties(), &PdfOptions::default())
            .unwrap(),
    )[0];
    assert!(
        !page.contains("E-Invoice Ready"),
        "no footer note expected in:\n{page}"
    );
    assert!(page.contains("Page 1 of 1"));
}

#[test]
fn test_footer_note_is_printed_when_given() {
    let custom = PdfOptions {
        footer_note: Some("Thank you for your business".into()),
        ..PdfOptions::default()
    };
    let pdf = invoice(42).to_pdf(&parties(), &custom).unwrap();
    for page in pages(&pdf) {
        assert!(
            page.contains("Thank you for your business"),
            "custom note missing in:\n{page}"
        );
    }
}

#[test]
fn test_walk_in_buyer_renders_with_a_placeholder_name() {
    let invoice = GstInvoice::new(
        "INV/2025-26/044",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        Recipient::unregistered(StateCode::parse("27").unwrap()),
        vec![item("Consulting")],
    )
    .unwrap();
    let parties = InvoiceParties::new(parties().seller, InvoiceParty::walk_in());
    let pdf = invoice.to_pdf(&parties, &PdfOptions::default()).unwrap();

    let page = &pages(&pdf)[0];
    for expected in [
        "Walk-in customer",
        "GSTIN: Unregistered",
        "Place of Supply: 27 - Maharashtra",
        "CGST",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
}

#[test]
fn test_bill_of_supply_renders_its_title_and_exempt_rates() {
    let exempt = GstLineItem::exempt(
        "4901",
        "Printed books",
        BigDecimal::from(3),
        "250".parse().unwrap(),
    )
    .unwrap();
    let invoice = GstInvoice::new(
        "INV/2025-26/045",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![exempt],
    )
    .unwrap();
    let pdf = invoice.to_pdf(&parties(), &PdfOptions::default()).unwrap();

    let page = &pages(&pdf)[0];
    for expected in ["Bill of Supply", "Exempt", "Printed books"] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
    assert!(
        !page.contains("Tax Invoice"),
        "no Tax Invoice title expected in:\n{page}"
    );
}

/// How many images the rendered PDF holds
fn image_count(pdf: &[u8]) -> usize {
    let document = PdfDocument::parse(pdf, &PdfParseOptions::default(), &mut Vec::new()).unwrap();
    document.resources.xobjects.map.len()
}

#[test]
fn test_invoice_without_logo_prints_no_logo() {
    let pdf = invoice(1)
        .to_pdf(&parties(), &PdfOptions::default())
        .unwrap();

    let page = &pages(&pdf)[0];
    assert!(
        !page.contains("LOGO"),
        "no logo placeholder expected in:\n{page}"
    );
    assert_eq!(image_count(&pdf), 0);
}

#[test]
fn test_logo_is_drawn_when_given() {
    let options = PdfOptions {
        logo: Some(LOGO_PNG.to_vec()),
        ..PdfOptions::default()
    };
    let pdf = invoice(1).to_pdf(&parties(), &options).unwrap();

    assert_eq!(image_count(&pdf), 1);
    assert!(pages(&pdf)[0].contains("Acme Services Pvt Ltd"));
}

#[test]
fn test_logo_that_is_not_an_image_is_refused() {
    let options = PdfOptions {
        logo: Some(b"not an image".to_vec()),
        ..PdfOptions::default()
    };
    let result = invoice(1).to_pdf(&parties(), &options);

    assert!(
        matches!(result, Err(PdfError::InvalidLogo { .. })),
        "{result:?}"
    );
}

#[test]
fn test_discounts_are_printed_in_the_table_and_totals() {
    let discounted = item("Consulting, discounted")
        .with_discount(Discount::Percent(BigDecimal::from(10)))
        .unwrap();
    let invoice = GstInvoice::new(
        "INV/2025-26/043",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![discounted, item("Consulting, full price")],
    )
    .unwrap()
    .with_discount(Discount::Amount(BigDecimal::from(500)))
    .unwrap();
    let pdf = invoice.to_pdf(&parties(), &PdfOptions::default()).unwrap();

    let page = &pages(&pdf)[0];
    for expected in [
        "Disc.",
        "375.23",
        "Subtotal",
        "Rs. 7,129.27",
        "Discount",
        "Rs. -500.00",
        "Taxable Value",
        "Rs. 6,629.27",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
}

#[test]
fn test_invoice_without_discounts_has_no_discount_column() {
    let pdf = invoice(2)
        .to_pdf(&parties(), &PdfOptions::default())
        .unwrap();
    let page = &pages(&pdf)[0];
    assert!(!page.contains("Disc."));
    assert!(!page.contains("Subtotal"));
}

#[test]
fn test_quantity_is_printed_with_its_unit() {
    let invoice = GstInvoice::new(
        "INV/2025-26/044",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![item("Site visits").with_unit("NOS")],
    )
    .unwrap();
    let page = &pages(&invoice.to_pdf(&parties(), &PdfOptions::default()).unwrap())[0];
    assert!(page.contains("3 NOS"), "missing unit in:\n{page}");
}

fn details() -> InvoiceDetails {
    InvoiceDetails {
        due_date: NaiveDate::from_ymd_opt(2025, 7, 15),
        reference: Some("PO-7781".into()),
        ship_to: Some(InvoiceParty::unregistered(
            "Globex Warehouse",
            vec!["Hosur Road, Bengaluru 560068".into()],
        )),
        notes: vec!["Goods once sold are taken back within 7 days.".into()],
    }
}

#[test]
fn test_details_are_printed_in_the_header_and_above_the_terms() {
    let print = InvoicePrint::from_invoice(&invoice(2), &parties())
        .unwrap()
        .with_details(&details());
    let options = PdfOptions {
        terms: vec!["Payment due within 15 days.".into()],
        ..PdfOptions::default()
    };
    let page = &pages(&render_pdf(&print, &options).unwrap())[0];

    for expected in [
        "Due Date: 15-07-2025",
        "Reference: PO-7781",
        "BILL TO",
        "SHIP TO",
        "Globex Warehouse",
        "Hosur Road, Bengaluru 560068",
        "Notes",
        "Goods once sold are taken back within 7 days.",
        "Payment due within 15 days.",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
    assert!(
        !page.contains("Unregistered"),
        "the ship-to party prints no GSTIN line without one in:\n{page}"
    );
}

#[test]
fn test_long_place_of_supply_wraps_rather_than_being_cut_short() {
    let print = InvoicePrint {
        place_of_supply: "26 - Dadra and Nagar Haveli and Daman and Diu".into(),
        ..InvoicePrint::from_invoice(&invoice(1), &parties()).unwrap()
    };
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];

    assert!(
        page.contains("Dadra and Nagar Haveli"),
        "missing state in:\n{page}"
    );
    assert!(page.contains("Diu"), "state name cut short in:\n{page}");
    assert!(!page.contains("..."), "unexpected ellipsis in:\n{page}");
}

#[test]
fn test_invoice_without_a_delivery_address_prints_no_ship_to() {
    let page = &pages(
        &invoice(1)
            .to_pdf(&parties(), &PdfOptions::default())
            .unwrap(),
    )[0];
    assert!(!page.contains("SHIP TO"));
    assert!(!page.contains("Due Date"));
    assert!(!page.contains("Notes"));
}

#[test]
fn test_notes_count_towards_the_room_below_the_totals() {
    let terms_only = PdfOptions {
        terms: vec!["Term.".into(); 200],
        ..PdfOptions::default()
    };
    let Err(PdfError::TermsTooLong { max_lines, .. }) = invoice(1).to_pdf(&parties(), &terms_only)
    else {
        panic!("expected the terms to be too long");
    };
    let options = PdfOptions {
        terms: vec!["Term.".into(); max_lines],
        ..PdfOptions::default()
    };
    let print = InvoicePrint::from_invoice(&invoice(1), &parties())
        .unwrap()
        .with_details(&details());

    let result = render_pdf(&print, &options);
    assert!(
        matches!(result, Err(PdfError::TermsTooLong { lines, max_lines: fit }) if lines == max_lines + 1 && fit < max_lines),
        "{result:?}"
    );
}

/// The print of `invoice` as a bill of supply that charges no tax, with `declaration`
fn no_tax_print(invoice: &GstInvoice, declaration: Option<&str>) -> InvoicePrint {
    InvoicePrint {
        charges_tax: false,
        declaration: declaration.map(str::to_string),
        ..InvoicePrint::from_invoice(invoice, &parties()).unwrap()
    }
}

/// An invoice of one exempt line of printed books, with `discount` off the line when given
fn exempt_invoice(discount: Option<Discount>) -> GstInvoice {
    let line = GstLineItem::exempt(
        "4901",
        "Printed books",
        BigDecimal::from(3),
        "250".parse().unwrap(),
    )
    .unwrap();
    let line = match discount {
        Some(discount) => line.with_discount(discount).unwrap(),
        None => line,
    };
    GstInvoice::new(
        "BOS/2025-26/001",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        gstin(SELLER),
        gstin(BUYER),
        vec![line],
    )
    .unwrap()
}

const DECLARATION: &str = "Composition taxable person, not eligible to collect tax on supplies";

#[test]
fn test_bill_of_supply_without_tax_prints_no_tax_columns_or_summary() {
    let print = no_tax_print(&exempt_invoice(None), None);
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];

    for expected in [
        "Bill of Supply",
        "Place of Supply",
        "Amount",
        "Total",
        "750.00",
    ] {
        assert!(
            page.contains(expected),
            "missing {expected:?} in:
{page}"
        );
    }
    for unexpected in [
        "Taxable Value",
        "Total Tax",
        "Intra-state",
        "Inter-state",
        "CGST",
        "IGST",
        "Exempt",
        "Declaration",
    ] {
        assert!(
            !page.contains(unexpected),
            "unexpected {unexpected:?} in:
{page}"
        );
    }
}

#[test]
fn test_bill_of_supply_with_line_discount_keeps_the_discount_column() {
    let discounted = exempt_invoice(Some(Discount::Amount(BigDecimal::from(50))));
    let print = no_tax_print(&discounted, None);
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];

    for expected in ["Disc.", "50.00", "Subtotal", "700.00"] {
        assert!(
            page.contains(expected),
            "missing {expected:?} in:
{page}"
        );
    }
    assert!(
        !page.contains("Taxable Value"),
        "in:
{page}"
    );
}

#[test]
fn test_declaration_is_printed_above_the_payment_details() {
    let print = no_tax_print(&exempt_invoice(None), Some(DECLARATION)).with_payment(payment());
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];

    let declaration = page.find(DECLARATION).expect("declaration printed");
    let heading = page.find("Declaration").expect("declaration heading");
    let payment = page.find("Payment details").expect("payment details");
    assert!(
        heading < declaration && declaration < payment,
        "in:
{page}"
    );
}

#[test]
fn test_declaration_counts_towards_the_room_below_the_totals() {
    let terms_only = PdfOptions {
        terms: vec!["Term.".into(); 200],
        ..PdfOptions::default()
    };
    let print = no_tax_print(&exempt_invoice(None), None);
    let Err(PdfError::TermsTooLong { max_lines, .. }) = render_pdf(&print, &terms_only) else {
        panic!("expected the terms to be too long");
    };
    let options = PdfOptions {
        terms: vec!["Term.".into(); max_lines],
        ..PdfOptions::default()
    };
    assert!(render_pdf(&print, &options).is_ok());

    let declared = no_tax_print(&exempt_invoice(None), Some(DECLARATION));
    let result = render_pdf(&declared, &options);
    assert!(
        matches!(result, Err(PdfError::TermsTooLong { lines, .. }) if lines == max_lines + 1),
        "{result:?}"
    );
}

#[test]
fn test_unregistered_sellers_bill_of_supply_prints_its_state_and_declaration() {
    let seller = SellerRegistration::Unregistered {
        state: StateCode::parse("29").unwrap(),
    };
    let line = GstLineItem::new(
        "998314",
        "Tailoring",
        BigDecimal::from(2),
        "400".parse().unwrap(),
        BigDecimal::from(0),
    )
    .unwrap();
    let invoice = GstInvoice::new(
        "BOS/2025-26/002",
        NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        seller,
        Recipient::unregistered(StateCode::parse("29").unwrap()),
        vec![line],
    )
    .unwrap();
    let parties = InvoiceParties::new(
        InvoiceParty::unregistered("Ravi Tailors", vec!["Jayanagar, Bengaluru".into()]),
        InvoiceParty::walk_in(),
    );
    let page = &pages(&invoice.to_pdf(&parties, &PdfOptions::default()).unwrap())[0];

    for expected in [
        "Bill of Supply",
        "State: 29 - Karnataka",
        "Declaration",
        UNREGISTERED_DECLARATION,
        "Rs. 800.00",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
    for unexpected in ["Taxable Value", "Total Tax", "Intra-state", "18%"] {
        assert!(
            !page.contains(unexpected),
            "unexpected {unexpected:?} in:\n{page}"
        );
    }
}

fn payment() -> PaymentDetails {
    PaymentDetails {
        bank_name: Some("HDFC Bank".into()),
        account_number: Some("50100012345678".into()),
        ifsc: Some("HDFC0001234".into()),
        upi_id: Some("acme@okhdfc".into()),
        ..PaymentDetails::default()
    }
}

#[test]
fn test_payment_details_are_printed_on_the_last_page() {
    let print = InvoicePrint::from_invoice(&invoice(2), &parties())
        .unwrap()
        .with_payment(payment());
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];

    for expected in [
        "Payment details",
        "Name: Acme Services Pvt Ltd",
        "Bank: HDFC Bank",
        "A/c No: 50100012345678",
        "IFSC: HDFC0001234",
        "UPI: acme@okhdfc",
        "Scan to pay with any UPI app",
    ] {
        assert!(page.contains(expected), "missing {expected:?} in:\n{page}");
    }
}

#[test]
fn test_no_qr_code_without_a_upi_link() {
    let bank_only = PaymentDetails {
        upi_id: None,
        ..payment()
    };
    let print = InvoicePrint::from_invoice(&invoice(1), &parties())
        .unwrap()
        .with_payment(bank_only);
    let page = &pages(&render_pdf(&print, &PdfOptions::default()).unwrap())[0];
    assert!(page.contains("IFSC: HDFC0001234"));
    assert!(
        !page.contains("Scan to pay"),
        "no QR caption expected in:\n{page}"
    );
}

#[test]
fn test_invoice_without_payment_details_prints_no_payment_block() {
    let page = &pages(
        &invoice(1)
            .to_pdf(&parties(), &PdfOptions::default())
            .unwrap(),
    )[0];
    assert!(!page.contains("Payment details"));
}

#[test]
fn test_long_invoice_keeps_totals_and_payment_together_on_the_last_page() {
    let options = PdfOptions {
        terms: vec!["Payment due within 15 days.".into(); 3],
        ..PdfOptions::default()
    };
    let render = |print: &InvoicePrint| pages(&render_pdf(print, &options).unwrap());
    for lines in [20, 30, 40, 55] {
        let print = InvoicePrint::from_invoice(&invoice(lines), &parties())
            .unwrap()
            .with_details(&details())
            .with_payment(payment());
        let pages = render(&print);
        let last = pages.last().unwrap();
        for expected in [
            "Amount in words",
            "Payment details",
            "IFSC: HDFC0001234",
            "Authorised Signatory",
        ] {
            assert!(
                last.contains(expected),
                "{lines} lines: missing {expected:?} in:\n{last}"
            );
        }
        let printed = pages
            .iter()
            .flat_map(|page| page.lines())
            .filter(|line| line.starts_with("Consulting, phase "))
            .count();
        assert_eq!(printed, lines);
    }
}
