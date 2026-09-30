//! Render a GST invoice to PDF
//!
//! Run with `cargo run --example gst_invoice_pdf --features pdf`. The PDF is written to the
//! system temp directory. Pass the path of a TrueType font that has the ₹ glyph (for example
//! Noto Sans) as the first argument to print amounts with the rupee sign.

use accounting_core::invoice::{
    GstInvoice, GstLineItem, Gstin, InvoiceParties, InvoiceParty, PdfFont, PdfOptions,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

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
        )?,
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
    )?;

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

    let pdf = invoice.to_pdf(&parties, &options)?;
    let path = std::env::temp_dir().join("gst_invoice_example.pdf");
    std::fs::write(&path, &pdf)?;
    println!("Wrote {} bytes to {}", pdf.len(), path.display());
    Ok(())
}
