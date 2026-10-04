//! GSTR-1 example: aggregate a month of B2B, B2CL and B2CS invoices and a credit note into the
//! return of outward supplies and export it as JSON in the GST portal's offline-tool schema

use accounting_core::invoice::{
    CreditNote, GstInvoice, GstLineItem, Gstin, HsnMaster, Recipient, StateCode,
};
use accounting_core::returns::{Gstr1Return, ReturnPeriod};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - GSTR-1 Export\n");

    let seller = Gstin::parse("27AAPFU0939F1ZV")?; // Maharashtra
    let local_buyer = Gstin::parse("27AAPFU0939F2ZU")?; // Maharashtra
    let remote_buyer = Gstin::parse("29AAPFU0939F1ZR")?; // Karnataka
    let retail_buyer = Recipient::unregistered(StateCode::parse("07")?); // Delhi, no GSTIN
    let walk_in_local = Recipient::unregistered(StateCode::parse("27")?); // Maharashtra
    let walk_in_remote = Recipient::unregistered(StateCode::parse("29")?); // Karnataka
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
        // Inter-state to an unregistered buyer, over the ₹1 lakh threshold: B2CL
        GstInvoice::new(
            "INV/24-25/104",
            date(28)?,
            seller.clone(),
            retail_buyer,
            vec![line("847130", "Laptop", 1, "95000", 18)?],
        )?,
        // Walk-in sales to unregistered buyers: intra-state, and inter-state under ₹1 lakh: B2CS
        GstInvoice::new(
            "INV/24-25/105",
            date(29)?,
            seller.clone(),
            walk_in_local,
            vec![
                line("1905", "Biscuits", 40, "12.50", 5)?,
                // Nil-rated, exempt and non-GST: reported in Table 8, not Table 7
                line("4901", "Printed manuals", 2, "150", 0)?,
                GstLineItem::exempt(
                    "0401",
                    "Fresh milk",
                    BigDecimal::from(10),
                    BigDecimal::from(60),
                )?,
                GstLineItem::non_gst("2710", "Petrol", BigDecimal::from(5), BigDecimal::from(100))?,
            ],
        )?,
        GstInvoice::new(
            "INV/24-25/106",
            date(30)?,
            seller.clone(),
            walk_in_remote,
            vec![line("998314", "IT consulting", 2, "1500", 18)?],
        )?,
    ];

    // The Karnataka buyer returns one of the two laptops on INV/24-25/102
    let credit_notes = vec![CreditNote::new(
        &invoices[1],
        "CN/24-25/001",
        date(20)?,
        vec![line("847130", "Laptop returned", 1, "55000", 18)?],
    )?];

    let period = ReturnPeriod::new(2024, 11)?;
    let gstr1 = Gstr1Return::build(
        &seller,
        period,
        &invoices,
        &credit_notes,
        HsnMaster::global(),
    )?;

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
    for place in &gstr1.b2cl {
        println!(
            "  🛒 Unregistered buyers in state {}: {} B2CL invoice(s)",
            place.place_of_supply,
            place.invoices.len()
        );
    }
    for row in &gstr1.b2cs {
        println!(
            "  🧺 B2CS {:?} to state {} at {}%: taxable {}",
            row.supply_type, row.place_of_supply, row.rate, row.taxable_value
        );
    }
    for party in &gstr1.cdnr {
        for note in &party.notes {
            println!(
                "  ↩️  Credit note {} to {} on {}: value {}",
                note.note_number, party.buyer_gstin, note.note_date, note.note_value
            );
        }
    }
    for row in &gstr1.nil.rows {
        println!(
            "  🆓 Table 8 {:?}: nil-rated {}, exempt {}, non-GST {}",
            row.supply_type, row.nil_rated, row.exempt, row.non_gst
        );
    }
    println!(
        "  📦 HSN summary rows: {} B2B, {} B2C",
        gstr1.hsn.b2b.len(),
        gstr1.hsn.b2c.len()
    );

    println!("\n📤 Portal JSON:\n{}", gstr1.to_json()?);
    Ok(())
}
