//! GST (Goods and Services Tax) calculation engine for Indian tax compliance

use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// GST rate structure for Indian taxation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GstRate {
    /// Total GST rate percentage (e.g., 18.0 for 18%)
    pub total_rate: BigDecimal,
    /// CGST rate percentage (Central GST)
    pub cgst_rate: BigDecimal,
    /// SGST rate percentage (State GST)
    pub sgst_rate: BigDecimal,
    /// IGST rate percentage (Integrated GST)
    pub igst_rate: BigDecimal,
}

impl GstRate {
    /// Create a new GST rate with intra-state rates (CGST + SGST)
    pub fn intra_state(total_rate: BigDecimal) -> Self {
        let half_rate = &total_rate / BigDecimal::from(2);
        Self {
            total_rate,
            cgst_rate: half_rate.clone(),
            sgst_rate: half_rate,
            igst_rate: BigDecimal::from(0),
        }
    }

    /// Create a new GST rate with inter-state rates (IGST)
    pub fn inter_state(total_rate: BigDecimal) -> Self {
        Self {
            total_rate: total_rate.clone(),
            cgst_rate: BigDecimal::from(0),
            sgst_rate: BigDecimal::from(0),
            igst_rate: total_rate,
        }
    }

    /// Validate that the GST rate structure is correct
    pub fn validate(&self) -> Result<(), GstError> {
        let calculated_total = &self.cgst_rate + &self.sgst_rate + &self.igst_rate;

        if calculated_total != self.total_rate {
            return Err(GstError::InvalidRate(format!(
                "GST components don't add up to total rate: {} != {}",
                calculated_total, self.total_rate
            )));
        }

        // For intra-state transactions, CGST and SGST should be equal
        if self.igst_rate == BigDecimal::from(0) && self.cgst_rate != self.sgst_rate {
            return Err(GstError::InvalidRate(
                "CGST and SGST rates must be equal for intra-state transactions".to_string(),
            ));
        }

        // For inter-state transactions, only IGST should be non-zero
        if self.igst_rate > BigDecimal::from(0)
            && (self.cgst_rate > BigDecimal::from(0) || self.sgst_rate > BigDecimal::from(0))
        {
            return Err(GstError::InvalidRate(
                "Only IGST should be applicable for inter-state transactions".to_string(),
            ));
        }

        Ok(())
    }
}

/// Detailed GST calculation breakdown
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GstCalculation {
    /// Base amount (before GST)
    pub base_amount: BigDecimal,
    /// GST rate used for calculation
    pub gst_rate: GstRate,
    /// Calculated CGST amount
    pub cgst_amount: BigDecimal,
    /// Calculated SGST amount
    pub sgst_amount: BigDecimal,
    /// Calculated IGST amount
    pub igst_amount: BigDecimal,
    /// Total GST amount (CGST + SGST + IGST)
    pub total_gst_amount: BigDecimal,
    /// Total amount including GST
    pub total_amount: BigDecimal,
}

impl GstCalculation {
    /// Calculate GST amounts from base amount and GST rate
    pub fn calculate(base_amount: BigDecimal, gst_rate: GstRate) -> Result<Self, GstError> {
        gst_rate.validate()?;

        let cgst_amount = (&base_amount * &gst_rate.cgst_rate) / BigDecimal::from(100);
        let sgst_amount = (&base_amount * &gst_rate.sgst_rate) / BigDecimal::from(100);
        let igst_amount = (&base_amount * &gst_rate.igst_rate) / BigDecimal::from(100);

        let total_gst_amount = &cgst_amount + &sgst_amount + &igst_amount;
        let total_amount = &base_amount + &total_gst_amount;

        Ok(Self {
            base_amount,
            gst_rate,
            cgst_amount,
            sgst_amount,
            igst_amount,
            total_gst_amount,
            total_amount,
        })
    }

    /// Calculate base amount from total amount (reverse calculation)
    pub fn reverse_calculate(
        total_amount: BigDecimal,
        gst_rate: GstRate,
    ) -> Result<Self, GstError> {
        gst_rate.validate()?;

        let divisor = BigDecimal::from(100) + &gst_rate.total_rate;
        let base_amount = (&total_amount * BigDecimal::from(100)) / divisor;

        Self::calculate(base_amount, gst_rate)
    }
}

/// Standard GST rates for different categories of goods and services
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GstCategory {
    /// Essential items (food, medicines, etc.) - 0%
    Essential,
    /// Reduced rate items - 5%
    Reduced,
    /// Standard rate items - 12%
    Standard,
    /// Higher rate items - 18%
    Higher,
    /// Luxury/Sin goods - 28%
    Luxury,
}

impl GstCategory {
    /// Get the standard GST rate for this category
    pub fn rate(&self) -> BigDecimal {
        match self {
            GstCategory::Essential => BigDecimal::from(0),
            GstCategory::Reduced => BigDecimal::from(5),
            GstCategory::Standard => BigDecimal::from(12),
            GstCategory::Higher => BigDecimal::from(18),
            GstCategory::Luxury => BigDecimal::from(28),
        }
    }

    /// Create intra-state GST rate for this category
    pub fn intra_state_rate(&self) -> GstRate {
        GstRate::intra_state(self.rate())
    }

    /// Create inter-state GST rate for this category
    pub fn inter_state_rate(&self) -> GstRate {
        GstRate::inter_state(self.rate())
    }
}

/// GST calculation engine
#[derive(Debug)]
pub struct GstCalculator {
    /// Standard category rates
    category_rates: HashMap<GstCategory, GstRate>,
    /// Custom product/service specific rates
    custom_rates: HashMap<String, GstRate>,
    /// Default transaction type (intra-state or inter-state)
    default_is_inter_state: bool,
}

impl GstCalculator {
    /// Create a new GST calculator
    pub fn new(default_is_inter_state: bool) -> Self {
        let mut calculator = Self {
            category_rates: HashMap::new(),
            custom_rates: HashMap::new(),
            default_is_inter_state,
        };

        calculator.setup_standard_rates();
        calculator
    }

    /// Setup standard GST rates for all categories
    fn setup_standard_rates(&mut self) {
        let categories = [
            GstCategory::Essential,
            GstCategory::Reduced,
            GstCategory::Standard,
            GstCategory::Higher,
            GstCategory::Luxury,
        ];

        for category in categories.iter() {
            let rate = if self.default_is_inter_state {
                category.inter_state_rate()
            } else {
                category.intra_state_rate()
            };
            self.category_rates.insert(*category, rate);
        }
    }

    /// Set a custom GST rate for a specific product/service
    pub fn set_custom_rate(
        &mut self,
        product_code: String,
        gst_rate: GstRate,
    ) -> Result<(), GstError> {
        gst_rate.validate()?;
        self.custom_rates.insert(product_code, gst_rate);
        Ok(())
    }

    /// Calculate GST for a product using category rates
    pub fn calculate_by_category(
        &self,
        base_amount: BigDecimal,
        category: GstCategory,
        is_inter_state: Option<bool>,
    ) -> Result<GstCalculation, GstError> {
        let gst_rate = match is_inter_state.unwrap_or(self.default_is_inter_state) {
            true => category.inter_state_rate(),
            false => category.intra_state_rate(),
        };

        GstCalculation::calculate(base_amount, gst_rate)
    }

    /// Calculate GST for a product using custom rates
    pub fn calculate_by_product(
        &self,
        base_amount: BigDecimal,
        product_code: &str,
    ) -> Result<GstCalculation, GstError> {
        let gst_rate = self
            .custom_rates
            .get(product_code)
            .ok_or_else(|| GstError::ProductNotFound(product_code.to_string()))?;

        GstCalculation::calculate(base_amount, gst_rate.clone())
    }

    /// Calculate GST with explicit rate
    pub fn calculate_with_rate(
        &self,
        base_amount: BigDecimal,
        gst_rate: GstRate,
    ) -> Result<GstCalculation, GstError> {
        GstCalculation::calculate(base_amount, gst_rate)
    }

    /// Reverse calculate base amount from total
    pub fn reverse_calculate_by_category(
        &self,
        total_amount: BigDecimal,
        category: GstCategory,
        is_inter_state: Option<bool>,
    ) -> Result<GstCalculation, GstError> {
        let gst_rate = match is_inter_state.unwrap_or(self.default_is_inter_state) {
            true => category.inter_state_rate(),
            false => category.intra_state_rate(),
        };

        GstCalculation::reverse_calculate(total_amount, gst_rate)
    }
}

/// Invoice line item with GST calculation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GstLineItem {
    /// Item description
    pub description: String,
    /// Quantity
    pub quantity: BigDecimal,
    /// Unit price (before GST)
    pub unit_price: BigDecimal,
    /// Line total before GST
    pub line_total_before_gst: BigDecimal,
    /// GST calculation for this line
    pub gst_calculation: GstCalculation,
    /// Line total including GST
    pub line_total_with_gst: BigDecimal,
}

impl GstLineItem {
    /// Create a new line item with GST calculation
    pub fn new(
        description: String,
        quantity: BigDecimal,
        unit_price: BigDecimal,
        gst_rate: GstRate,
    ) -> Result<Self, GstError> {
        let line_total_before_gst = &quantity * &unit_price;
        let gst_calculation = GstCalculation::calculate(line_total_before_gst.clone(), gst_rate)?;
        let line_total_with_gst = gst_calculation.total_amount.clone();

        Ok(Self {
            description,
            quantity,
            unit_price,
            line_total_before_gst,
            gst_calculation,
            line_total_with_gst,
        })
    }
}

/// Complete GST invoice calculation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GstInvoice {
    /// Invoice line items
    pub line_items: Vec<GstLineItem>,
    /// Total before GST
    pub total_before_gst: BigDecimal,
    /// Total CGST amount
    pub total_cgst: BigDecimal,
    /// Total SGST amount
    pub total_sgst: BigDecimal,
    /// Total IGST amount
    pub total_igst: BigDecimal,
    /// Total GST amount
    pub total_gst: BigDecimal,
    /// Grand total including GST
    pub grand_total: BigDecimal,
}

impl GstInvoice {
    /// Create a new GST invoice from line items
    pub fn new(line_items: Vec<GstLineItem>) -> Self {
        let total_before_gst: BigDecimal = line_items
            .iter()
            .map(|item| &item.line_total_before_gst)
            .sum();

        let total_cgst: BigDecimal = line_items
            .iter()
            .map(|item| &item.gst_calculation.cgst_amount)
            .sum();

        let total_sgst: BigDecimal = line_items
            .iter()
            .map(|item| &item.gst_calculation.sgst_amount)
            .sum();

        let total_igst: BigDecimal = line_items
            .iter()
            .map(|item| &item.gst_calculation.igst_amount)
            .sum();

        let total_gst = &total_cgst + &total_sgst + &total_igst;
        let grand_total = &total_before_gst + &total_gst;

        Self {
            line_items,
            total_before_gst,
            total_cgst,
            total_sgst,
            total_igst,
            total_gst,
            grand_total,
        }
    }

    /// Add a line item to the invoice
    pub fn add_line_item(&mut self, line_item: GstLineItem) {
        self.line_items.push(line_item);
        self.recalculate_totals();
    }

    /// Recalculate all totals after modifications
    fn recalculate_totals(&mut self) {
        let invoice = Self::new(self.line_items.clone());
        self.total_before_gst = invoice.total_before_gst;
        self.total_cgst = invoice.total_cgst;
        self.total_sgst = invoice.total_sgst;
        self.total_igst = invoice.total_igst;
        self.total_gst = invoice.total_gst;
        self.grand_total = invoice.grand_total;
    }
}

/// GST-related errors
#[derive(Debug, thiserror::Error)]
pub enum GstError {
    #[error("Invalid GST rate: {0}")]
    InvalidRate(String),
    #[error("Product not found: {0}")]
    ProductNotFound(String),
    #[error("Calculation error: {0}")]
    Calculation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gst_rate_intra_state() {
        let rate = GstRate::intra_state(BigDecimal::from(18));
        assert_eq!(rate.total_rate, BigDecimal::from(18));
        assert_eq!(rate.cgst_rate, BigDecimal::from(9));
        assert_eq!(rate.sgst_rate, BigDecimal::from(9));
        assert_eq!(rate.igst_rate, BigDecimal::from(0));
        assert!(rate.validate().is_ok());
    }

    #[test]
    fn test_gst_rate_inter_state() {
        let rate = GstRate::inter_state(BigDecimal::from(18));
        assert_eq!(rate.total_rate, BigDecimal::from(18));
        assert_eq!(rate.cgst_rate, BigDecimal::from(0));
        assert_eq!(rate.sgst_rate, BigDecimal::from(0));
        assert_eq!(rate.igst_rate, BigDecimal::from(18));
        assert!(rate.validate().is_ok());
    }

    #[test]
    fn test_gst_calculation() {
        let base_amount = BigDecimal::from(1000);
        let gst_rate = GstRate::intra_state(BigDecimal::from(18));

        let calculation = GstCalculation::calculate(base_amount, gst_rate).unwrap();

        assert_eq!(calculation.base_amount, BigDecimal::from(1000));
        assert_eq!(calculation.cgst_amount, BigDecimal::from(90));
        assert_eq!(calculation.sgst_amount, BigDecimal::from(90));
        assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
        assert_eq!(calculation.total_amount, BigDecimal::from(1180));
    }

    #[test]
    fn test_gst_reverse_calculation() {
        let total_amount = BigDecimal::from(1180);
        let gst_rate = GstRate::intra_state(BigDecimal::from(18));

        let calculation = GstCalculation::reverse_calculate(total_amount, gst_rate).unwrap();

        assert_eq!(calculation.total_amount, BigDecimal::from(1180));
        assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
        assert_eq!(calculation.base_amount, BigDecimal::from(1000));
    }

    #[test]
    fn test_gst_calculator() {
        let calculator = GstCalculator::new(false); // intra-state default

        let calculation = calculator
            .calculate_by_category(BigDecimal::from(1000), GstCategory::Higher, None)
            .unwrap();

        assert_eq!(calculation.total_gst_amount, BigDecimal::from(180));
        assert_eq!(calculation.cgst_amount, BigDecimal::from(90));
        assert_eq!(calculation.sgst_amount, BigDecimal::from(90));
    }

    #[test]
    fn test_gst_invoice() {
        let gst_rate = GstRate::intra_state(BigDecimal::from(18));

        let line_item1 = GstLineItem::new(
            "Product A".to_string(),
            BigDecimal::from(2),
            BigDecimal::from(500),
            gst_rate.clone(),
        )
        .unwrap();

        let line_item2 = GstLineItem::new(
            "Product B".to_string(),
            BigDecimal::from(1),
            BigDecimal::from(300),
            gst_rate,
        )
        .unwrap();

        let invoice = GstInvoice::new(vec![line_item1, line_item2]);

        assert_eq!(invoice.total_before_gst, BigDecimal::from(1300));
        assert_eq!(invoice.total_gst, BigDecimal::from(234)); // 18% of 1300
        assert_eq!(invoice.grand_total, BigDecimal::from(1534));
    }
}
