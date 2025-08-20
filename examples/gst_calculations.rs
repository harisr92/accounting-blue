//! GST calculation examples

use accounting_core::{
    GstCalculation, GstCalculator, GstCategory, GstInvoice, GstLineItem, GstRate,
};
use bigdecimal::BigDecimal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧾 Accounting Core - GST Calculation Examples\n");

    // 1. Basic GST calculations for different categories
    println!("📊 Standard GST Rates by Category:");
    let categories = [
        (GstCategory::Essential, "Essential items (medicines, food)"),
        (GstCategory::Reduced, "Reduced rate items"),
        (GstCategory::Standard, "Standard rate items"),
        (GstCategory::Higher, "Higher rate items (most services)"),
        (GstCategory::Luxury, "Luxury/Sin goods"),
    ];

    for (category, description) in categories.iter() {
        println!("  {:?}: {}% - {}", category, category.rate(), description);
    }
    println!();

    // 2. Intra-state vs Inter-state calculations
    println!("🏢 Intra-state Transaction (CGST + SGST):");
    let intra_state_calculator = GstCalculator::new(false);
    let base_amount = BigDecimal::from(10000);

    let intra_state_calc = intra_state_calculator.calculate_by_category(
        base_amount.clone(),
        GstCategory::Higher,
        None,
    )?;

    println!("  Base Amount: ₹{}", intra_state_calc.base_amount);
    println!("  CGST (9%):   ₹{}", intra_state_calc.cgst_amount);
    println!("  SGST (9%):   ₹{}", intra_state_calc.sgst_amount);
    println!("  IGST:        ₹{}", intra_state_calc.igst_amount);
    println!("  Total GST:   ₹{}", intra_state_calc.total_gst_amount);
    println!("  Final Total: ₹{}", intra_state_calc.total_amount);
    println!();

    println!("🌍 Inter-state Transaction (IGST only):");
    let inter_state_calc = intra_state_calculator.calculate_by_category(
        base_amount.clone(),
        GstCategory::Higher,
        Some(true), // force inter-state
    )?;

    println!("  Base Amount: ₹{}", inter_state_calc.base_amount);
    println!("  CGST:        ₹{}", inter_state_calc.cgst_amount);
    println!("  SGST:        ₹{}", inter_state_calc.sgst_amount);
    println!("  IGST (18%):  ₹{}", inter_state_calc.igst_amount);
    println!("  Total GST:   ₹{}", inter_state_calc.total_gst_amount);
    println!("  Final Total: ₹{}", inter_state_calc.total_amount);
    println!();

    // 3. Reverse calculation (from total to base)
    println!("🔄 Reverse Calculation (Total to Base):");
    let total_amount = BigDecimal::from(11800); // includes 18% GST
    let reverse_calc = GstCalculation::reverse_calculate(
        total_amount.clone(),
        GstRate::intra_state(BigDecimal::from(18)),
    )?;

    println!("  Given Total: ₹{}", total_amount);
    println!("  Base Amount: ₹{}", reverse_calc.base_amount);
    println!("  GST Amount:  ₹{}", reverse_calc.total_gst_amount);
    println!("  CGST:        ₹{}", reverse_calc.cgst_amount);
    println!("  SGST:        ₹{}", reverse_calc.sgst_amount);
    println!();

    // 4. Complex invoice with multiple line items
    println!("🧾 Multi-item Invoice with Different GST Rates:");

    let mut line_items = Vec::new();

    // Item 1: Essential goods (0% GST)
    let item1 = GstLineItem::new(
        "Rice - 10kg".to_string(),
        BigDecimal::from(2),
        BigDecimal::from(150),
        GstCategory::Essential.intra_state_rate(),
    )?;
    line_items.push(item1);

    // Item 2: Reduced rate (5% GST)
    let item2 = GstLineItem::new(
        "Coffee powder - 500g".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(400),
        GstCategory::Reduced.intra_state_rate(),
    )?;
    line_items.push(item2);

    // Item 3: Standard rate (12% GST)
    let item3 = GstLineItem::new(
        "Cooking oil - 1L".to_string(),
        BigDecimal::from(3),
        BigDecimal::from(120),
        GstCategory::Standard.intra_state_rate(),
    )?;
    line_items.push(item3);

    // Item 4: Higher rate (18% GST)
    let item4 = GstLineItem::new(
        "Consultation service".to_string(),
        BigDecimal::from(1),
        BigDecimal::from(2000),
        GstCategory::Higher.intra_state_rate(),
    )?;
    line_items.push(item4);

    let invoice = GstInvoice::new(line_items);

    println!("  Line Items:");
    for (i, item) in invoice.line_items.iter().enumerate() {
        println!(
            "    {}. {} × {} @ ₹{} = ₹{} (GST: ₹{})",
            i + 1,
            item.description,
            item.quantity,
            item.unit_price,
            item.line_total_before_gst,
            item.gst_calculation.total_gst_amount
        );
    }
    println!();

    println!("  Invoice Summary:");
    println!("    Subtotal (before GST): ₹{}", invoice.total_before_gst);
    println!("    Total CGST:            ₹{}", invoice.total_cgst);
    println!("    Total SGST:            ₹{}", invoice.total_sgst);
    println!("    Total IGST:            ₹{}", invoice.total_igst);
    println!("    Total GST:             ₹{}", invoice.total_gst);
    println!("    Grand Total:           ₹{}", invoice.grand_total);
    println!();

    // 5. Custom GST rates
    println!("⚙️ Custom GST Rate Example:");
    let mut calculator = GstCalculator::new(false);

    // Add a custom rate for a specific product (e.g., special economic zone)
    let custom_rate = GstRate::intra_state(BigDecimal::from(12));
    calculator.set_custom_rate("PRODUCT_SEZ_001".to_string(), custom_rate)?;

    let custom_calc = calculator.calculate_by_product(BigDecimal::from(5000), "PRODUCT_SEZ_001")?;

    println!("  Product: PRODUCT_SEZ_001");
    println!("  Base Amount: ₹{}", custom_calc.base_amount);
    println!("  Custom GST:  ₹{} (12%)", custom_calc.total_gst_amount);
    println!("  Total:       ₹{}", custom_calc.total_amount);
    println!();

    // 6. Validation examples
    println!("✅ GST Rate Validation:");

    // Valid rate
    let valid_rate = GstRate::intra_state(BigDecimal::from(18));
    match valid_rate.validate() {
        Ok(()) => println!("  ✓ Valid intra-state rate: CGST 9% + SGST 9% = 18%"),
        Err(e) => println!("  ❌ Invalid rate: {}", e),
    }

    // Invalid rate (components don't add up)
    let invalid_rate = GstRate {
        total_rate: BigDecimal::from(18),
        cgst_rate: BigDecimal::from(10), // Should be 9
        sgst_rate: BigDecimal::from(9),
        igst_rate: BigDecimal::from(0),
    };
    match invalid_rate.validate() {
        Ok(()) => println!("  ✓ Valid rate"),
        Err(e) => println!("  ❌ Invalid rate: {}", e),
    }

    println!("\n🎉 GST calculation examples completed successfully!");
    Ok(())
}
