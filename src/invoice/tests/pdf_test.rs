use crate::invoice::pdf::*;
use crate::invoice::print::{InvoiceParties, InvoiceParty};
use crate::invoice::types::{GstInvoice, GstLineItem, Gstin, InvoiceError};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use printpdf::{BuiltinFont, PdfDocument, PdfParseOptions};

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER: &str = "29AAPFU0939F1ZR";

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
        "Consulting, phase 2",
        "IGST",
        "Rs. 8,855.32",
        "Rupees Eight Thousand Eight Hundred Fifty Five and Thirty Two Paise Only",
        "Payment due within 30 days.",
        "Authorised Signatory",
        "E-Invoice Ready",
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
    parties.buyer.gstin = gstin(SELLER);
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
    // below can be drawn and only the renderer's own labels ("LOGO", "Tax Invoice", ...) can't
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
        footer_note: String::new(),
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
