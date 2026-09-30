//! GSTR-1 example: aggregate a month of B2B invoices into the return of outward supplies and
//! export it as JSON in the GST portal's offline-tool schema

use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin, HsnMaster};
use accounting_core::returns::{Gstr1Return, ReturnPeriod};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - GSTR-1 Export\n");

    let seller = Gstin::parse("27AAPFU0939F1ZV")?; // Maharashtra
    let local_buyer = Gstin::parse("27AAPFU0939F2ZU")?; // Maharashtra
    let remote_buyer = Gstin::parse("29AAPFU0939F1ZR")?; // Karnataka
    let date = |day| NaiveDate::from_ymd_opt(2024, 11, day).ok_or("invalid date");
    let line = |hsn: &str, description: &str, quantity: u32, price: &str, rate: u32| {
        GstLineItem::new(
            hsn,
            description,
            BigDecimal::from(quantity),
            BigDecimal::from_str(price)?,
            BigDecimal::from(rate),
        )
        .map_err(Box::<dyn std::error::Error>::from)
    };

    let invoices = vec![
        GstInvoice::new(
            "INV/24-25/101",
            date(4)?,
            seller.clone(),
            local_buyer.clone(),
            vec![line("998314", "IT consulting", 10, "1500", 18)?],
        )?,
        GstInvoice::new(
            "INV/24-25/102",
            date(12)?,
            seller.clone(),
            remote_buyer,
            vec![
                line("847130", "Laptop", 2, "55000", 18)?,
                line("4901", "Printed manuals", 10, "150", 0)?,
            ],
        )?,
        GstInvoice::new(
            "INV/24-25/103",
            date(26)?,
            seller.clone(),
            local_buyer,
            vec![line("998314", "IT consulting", 4, "1500", 18)?],
        )?,
    ];

    let period = ReturnPeriod::new(2024, 11)?;
    let gstr1 = Gstr1Return::build(&seller, period, &invoices, HsnMaster::global())?;

    println!("📅 Period {} for {}", gstr1.period, gstr1.filer_gstin);
    for party in &gstr1.b2b {
        println!(
            "  👤 {}: {} invoice(s)",
            party.buyer_gstin,
            party.invoices.len()
        );
        for invoice in &party.invoices {
            println!(
                "     {} on {}: value {} in {} rate item(s)",
                invoice.invoice_number,
                invoice.invoice_date,
                invoice.invoice_value,
                invoice.items.len()
            );
        }
    }
    println!("  📦 HSN summary rows: {}", gstr1.hsn.b2b.len());

    println!("\n📤 Portal JSON:\n{}", gstr1.to_json()?);
    Ok(())
}
