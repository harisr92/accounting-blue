//! GST invoice examples: GSTIN validation, HSN/SAC rate lookup, intra-state and inter-state invoices

use accounting_core::invoice::{GstBreakdown, GstInvoice, GstLineItem, Gstin, HsnMaster};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - GST Invoice Examples\n");

    // 1. GSTIN validation
    println!("🔎 GSTIN Validation:");
    for candidate in [
        "27AAPFU0939F1ZV", // valid (Maharashtra)
        "27aapfu0939f1zv", // valid, normalised to uppercase
        "27AAPFU0939F1ZA", // wrong checksum
        "40AAPFU0939F1ZV", // unassigned state code
        "27AAPFU0939F1",   // too short
    ] {
        match Gstin::parse(candidate) {
            Ok(gstin) => println!(
                "  ✅ {candidate} → {gstin} (state {}, PAN {})",
                gstin.state_code(),
                gstin.pan()
            ),
            Err(e) => println!("  ❌ {e}"),
        }
    }
    println!();

    // 2. HSN/SAC master: default GST rates, with fallback from tariff items to their headings
    let master = HsnMaster::global();
    println!(
        "📚 HSN/SAC Lookup ({} codes, {} schedule from {}):",
        master.entries().len(),
        master.schedule(),
        master.effective_from()
    );
    for code in ["1001", "7010", "84713010", "998314", "0000"] {
        match master.lookup(code) {
            Some(entry) => println!(
                "  ✅ {code} → {} {:?} \"{}\" at {}%",
                entry.code, entry.kind, entry.description, entry.gst_rate
            ),
            None => println!("  ❌ {code} → not in the master"),
        }
    }
    println!();

    let seller = Gstin::parse("27AAPFU0939F1ZV")?;
    let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();

    let line_items = vec![
        // Rate filled in from the HSN/SAC master (18%)
        GstLineItem::with_default_rate(
            "847130".to_string(),
            "Laptop".to_string(),
            BigDecimal::from(2),
            BigDecimal::from(50000),
        )?,
        GstLineItem::new(
            "998314".to_string(),
            "Setup and configuration".to_string(),
            BigDecimal::from(1),
            BigDecimal::from(5000),
            BigDecimal::from(18),
        )?,
        GstLineItem::new(
            "4901".to_string(),
            "Printed user manual".to_string(),
            BigDecimal::from(2),
            BigDecimal::from(400),
            BigDecimal::from(5),
        )?,
    ];

    // 3. Intra-state supply: seller and buyer both in Maharashtra (27) → CGST + SGST
    let intra_state = GstInvoice::new(
        "INV/2024-25/001".to_string(),
        date,
        seller.clone(),
        Gstin::parse("27AAPFU0939F2ZU")?,
        line_items.clone(),
    )?;
    print_invoice("🏢 Intra-state Invoice (CGST + SGST)", &intra_state)?;

    // 4. Inter-state supply: buyer in Karnataka (29) → IGST
    let inter_state = GstInvoice::new(
        "INV/2024-25/002".to_string(),
        date,
        seller,
        Gstin::parse("29AAPFU0939F1ZR")?,
        line_items,
    )?;
    print_invoice("🌍 Inter-state Invoice (IGST)", &inter_state)?;

    // 5. Invalid invoices are rejected at construction
    println!("🚫 Rejected Inputs:");
    let too_long = GstInvoice::new(
        "INV/2024-25/00001".to_string(),
        date,
        inter_state.seller_gstin.clone(),
        inter_state.buyer_gstin.clone(),
        inter_state.line_items.clone(),
    );
    if let Err(e) = too_long {
        println!("  ❌ {e}");
    }
    let bad_hsn = GstLineItem::new(
        "847".to_string(),
        "Laptop".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(50000),
        BigDecimal::from(18),
    );
    if let Err(e) = bad_hsn {
        println!("  ❌ {e}");
    }
    let unknown_hsn = GstLineItem::with_default_rate(
        "0000".to_string(),
        "Mystery item".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(100),
    );
    if let Err(e) = unknown_hsn {
        println!("  ❌ {e}");
    }

    Ok(())
}

fn print_invoice(title: &str, invoice: &GstInvoice) -> Result<(), Box<dyn std::error::Error>> {
    println!("{title}:");
    println!("  Invoice No: {}", invoice.invoice_number);
    println!("  Date:       {}", invoice.invoice_date);
    println!("  Seller:     {}", invoice.seller_gstin);
    println!("  Buyer:      {}", invoice.buyer_gstin);
    println!("  Lines:");
    for (item, line) in invoice.line_items.iter().zip(invoice.line_breakdowns()?) {
        println!(
            "    [{}] {} × {} @ ₹{} ({}%) → taxable ₹{}, tax ₹{}",
            item.hsn_sac,
            item.description,
            item.quantity,
            item.unit_price,
            item.gst_rate,
            line.taxable_value,
            line.total_tax
        );
    }
    print_breakdown(&invoice.breakdown()?);
    println!();
    Ok(())
}

fn print_breakdown(breakdown: &GstBreakdown) {
    println!("  Taxable Value: ₹{}", breakdown.taxable_value);
    println!("  CGST:          ₹{}", breakdown.cgst);
    println!("  SGST:          ₹{}", breakdown.sgst);
    println!("  IGST:          ₹{}", breakdown.igst);
    println!("  Total Tax:     ₹{}", breakdown.total_tax);
    println!("  Grand Total:   ₹{}", breakdown.total);
}
