//! Render a GST invoice to PDF
//!
//! Run with `cargo run --example gst_invoice_pdf --features pdf`. Two PDFs are written to the
//! system temp directory: one without a logo, and one with the sample logo in
//! `examples/assets/logo.png`. Pass the path of a TrueType font that has the ₹ glyph (for example
//! Noto Sans) as the first argument to print amounts with the rupee sign.

use accounting_core::invoice::{
    Discount, GstInvoice, GstLineItem, Gstin, InvoiceParties, InvoiceParty, PdfFont, PdfOptions,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

/// Sample seller logo, a PNG; any PNG or JPEG works
const LOGO: &[u8] = include_bytes!("assets/logo.png");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seller = Gstin::parse("27AAPFU0939F1ZV")?;
    let buyer = Gstin::parse("29AAPFU0939F1ZR")?;

    let items = vec![
        GstLineItem::with_default_rate(
            "998314",
            "IT consulting - platform migration",
            BigDecimal::from(40),
            BigDecimal::from(2500),
        )?,
        GstLineItem::new(
            "8471",
            "Laptop, 14 inch, 16 GB RAM",
            BigDecimal::from(2),
            BigDecimal::from_str("68500.00")?,
            BigDecimal::from(18),
        )?
        // A line discount, printed in the Disc. column
        .with_discount(Discount::Percent(BigDecimal::from(5)))?,
        GstLineItem::new(
            "4901",
            "Printed training manuals",
            BigDecimal::from(25),
            BigDecimal::from_str("349.50")?,
            BigDecimal::from(5),
        )?,
    ];

    let invoice = GstInvoice::new(
        "INV/2025-26/042",
        NaiveDate::from_ymd_opt(2025, 6, 30).ok_or("invalid date")?,
        seller.clone(),
        buyer.clone(),
        items,
    )?
    // A discount off the subtotal, before tax, printed between the subtotal and taxable value
    .with_discount(Discount::Amount(BigDecimal::from(5000)))?;

    let parties = InvoiceParties::new(
        InvoiceParty::new(
            "Acme Services Pvt Ltd",
            vec![
                "4th Floor, Maker Chambers, Nariman Point".into(),
                "Mumbai, Maharashtra 400021".into(),
            ],
            seller,
        ),
        InvoiceParty::new(
            "Globex Technologies Ltd",
            vec!["12 MG Road".into(), "Bengaluru, Karnataka 560001".into()],
            buyer,
        ),
    );

    let mut options = PdfOptions {
        terms: vec![
            "Payment due within 30 days of the invoice date.".into(),
            "Interest at 18% p.a. is charged on overdue amounts.".into(),
        ],
        ..PdfOptions::default()
    };
    if let Some(path) = std::env::args().nth(1) {
        options.font = PdfFont::Custom {
            regular: std::fs::read(path)?,
            bold: None,
        };
        options.currency_label = "₹".into();
    }

    // Without a logo the seller block starts at the left margin
    write_pdf(
        &invoice.to_pdf(&parties, &options)?,
        "gst_invoice_example.pdf",
    )?;

    // With a logo it is drawn top left, scaled to fit, and the seller block moves right of it
    let with_logo = PdfOptions {
        logo: Some(LOGO.to_vec()),
        ..options
    };
    write_pdf(
        &invoice.to_pdf(&parties, &with_logo)?,
        "gst_invoice_example_logo.pdf",
    )?;
    Ok(())
}

/// Write `pdf` to `name` in the system temp directory
fn write_pdf(pdf: &[u8], name: &str) -> std::io::Result<()> {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, pdf)?;
    println!("Wrote {} bytes to {}", pdf.len(), path.display());
    Ok(())
}
