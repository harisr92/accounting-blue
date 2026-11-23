//! Render a GST invoice to PDF
//!
//! Run with `cargo run --example gst_invoice_pdf --features pdf`. Two PDFs are written to the
//! system temp directory: one without a logo, and one with the sample logo in
//! `examples/assets/logo.png`. Both show units on the lines, a due date, a reference, a delivery
//! address, notes, and payment details with a UPI QR code. Pass the path of a TrueType font that has the ₹ glyph (for example
//! Noto Sans) as the first argument to print amounts with the rupee sign.

use accounting_core::invoice::{
    render_pdf, Discount, GstInvoice, GstLineItem, Gstin, InvoiceDetails, InvoiceParties,
    InvoiceParty, InvoicePrint, PaymentDetails, PdfFont, PdfOptions,
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
        // The unit prints after the quantity, as in "2 NOS"
        .with_unit("NOS")
        // A line discount, printed in the Disc. column
        .with_discount(Discount::Percent(BigDecimal::from(5)))?,
        GstLineItem::new(
            "4901",
            "Printed training manuals",
            BigDecimal::from(25),
            BigDecimal::from_str("349.50")?,
            BigDecimal::from(5),
        )?
        .with_unit("PCS"),
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

    // Print-only details: due date, the buyer's reference, a delivery address and notes
    let details = InvoiceDetails {
        due_date: NaiveDate::from_ymd_opt(2025, 7, 30),
        reference: Some("PO-GLX-7781".into()),
        ship_to: Some(InvoiceParty::unregistered(
            "Globex Technologies - Warehouse",
            vec![
                "Plot 18, Hosur Road".into(),
                "Bengaluru, Karnataka 560068".into(),
            ],
        )),
        notes: vec!["Thank you for your business.".into()],
    };
    // Where to pay; the UPI ID adds a QR code for the invoice total
    let payment = PaymentDetails {
        bank_name: Some("HDFC Bank".into()),
        account_number: Some("50100012345678".into()),
        ifsc: Some("HDFC0001234".into()),
        upi_id: Some("acmeservices@okhdfc".into()),
        ..PaymentDetails::default()
    };
    let print = InvoicePrint::from_invoice(&invoice, &parties)?
        .with_details(&details)
        .with_payment(payment);

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
    write_pdf(&render_pdf(&print, &options)?, "gst_invoice_example.pdf")?;

    // With a logo it is drawn top left, scaled to fit, and the seller block moves right of it
    let with_logo = PdfOptions {
        logo: Some(LOGO.to_vec()),
        ..options
    };
    write_pdf(
        &render_pdf(&print, &with_logo)?,
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
