//! GST (Goods and Services Tax) calculation engine for Indian tax compliance

use bigdecimal::{BigDecimal, Signed, Zero};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// `rate` percent of `amount`
#[must_use]
pub fn percent_of(amount: &BigDecimal, rate: &BigDecimal) -> BigDecimal {
    (amount * rate) / BigDecimal::from(100)
}

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
    #[must_use]
    pub fn intra_state(total_rate: BigDecimal) -> Self {
        let half_rate = &total_rate / BigDecimal::from(2);
        Self {
            total_rate,
            cgst_rate: half_rate.clone(),
            sgst_rate: half_rate,
            igst_rate: BigDecimal::zero(),
        }
    }

    /// Create a new GST rate with inter-state rates (IGST)
    #[must_use]
    pub fn inter_state(total_rate: BigDecimal) -> Self {
        Self {
            igst_rate: total_rate.clone(),
            total_rate,
            cgst_rate: BigDecimal::zero(),
            sgst_rate: BigDecimal::zero(),
        }
    }

    /// IGST for an inter-state supply, CGST + SGST otherwise
    #[must_use]
    pub fn for_supply(total_rate: BigDecimal, is_inter_state: bool) -> Self {
        if is_inter_state {
            Self::inter_state(total_rate)
        } else {
            Self::intra_state(total_rate)
        }
    }

    /// Validate that the GST rate structure is correct
    ///
    /// # Errors
    ///
    /// [`GstError::ComponentsMismatch`] when the components don't add up to the total,
    /// [`GstError::UnequalSplit`] when CGST and SGST differ on an intra-state rate, and
    /// [`GstError::MixedIgstAndSplit`] when IGST is combined with CGST or SGST.
    pub fn validate(&self) -> Result<(), GstError> {
        let components = &self.cgst_rate + &self.sgst_rate + &self.igst_rate;
        if components != self.total_rate {
            return Err(GstError::ComponentsMismatch {
                components,
                total: self.total_rate.clone(),
            });
        }

        if self.igst_rate.is_zero() && self.cgst_rate != self.sgst_rate {
            return Err(GstError::UnequalSplit {
                cgst: self.cgst_rate.clone(),
                sgst: self.sgst_rate.clone(),
            });
        }

        if self.igst_rate.is_positive()
            && (self.cgst_rate.is_positive() || self.sgst_rate.is_positive())
        {
            return Err(GstError::MixedIgstAndSplit);
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
    ///
    /// # Errors
    ///
    /// Any error from [`GstRate::validate`].
    pub fn calculate(base_amount: BigDecimal, gst_rate: GstRate) -> Result<Self, GstError> {
        gst_rate.validate()?;

        let cgst_amount = percent_of(&base_amount, &gst_rate.cgst_rate);
        let sgst_amount = percent_of(&base_amount, &gst_rate.sgst_rate);
        let igst_amount = percent_of(&base_amount, &gst_rate.igst_rate);

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
    ///
    /// # Errors
    ///
    /// Any error from [`GstRate::validate`].
    #[allow(clippy::needless_pass_by_value)] // takes ownership to mirror `calculate`
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
    /// Every category, lowest rate first
    pub const ALL: [GstCategory; 5] = [
        GstCategory::Essential,
        GstCategory::Reduced,
        GstCategory::Standard,
        GstCategory::Higher,
        GstCategory::Luxury,
    ];

    /// Get the standard GST rate for this category
    #[must_use]
    pub fn rate(self) -> BigDecimal {
        let percent = match self {
            GstCategory::Essential => 0,
            GstCategory::Reduced => 5,
            GstCategory::Standard => 12,
            GstCategory::Higher => 18,
            GstCategory::Luxury => 28,
        };
        BigDecimal::from(percent)
    }

    /// Create intra-state GST rate for this category
    #[must_use]
    pub fn intra_state_rate(self) -> GstRate {
        GstRate::intra_state(self.rate())
    }

    /// Create inter-state GST rate for this category
    #[must_use]
    pub fn inter_state_rate(self) -> GstRate {
        GstRate::inter_state(self.rate())
    }

    /// GST rate for this category on an inter-state or intra-state supply
    #[must_use]
    pub fn rate_for_supply(self, is_inter_state: bool) -> GstRate {
        GstRate::for_supply(self.rate(), is_inter_state)
    }
}

/// GST calculator with a default supply type and optional per-product rates
#[derive(Debug, Clone, Default)]
pub struct GstCalculator {
    /// Custom product/service specific rates
    custom_rates: HashMap<String, GstRate>,
    /// Supply type used when a call does not say (inter-state or intra-state)
    default_is_inter_state: bool,
}

impl GstCalculator {
    /// Create a new GST calculator
    #[must_use]
    pub fn new(default_is_inter_state: bool) -> Self {
        Self {
            custom_rates: HashMap::new(),
            default_is_inter_state,
        }
    }

    fn category_rate(&self, category: GstCategory, is_inter_state: Option<bool>) -> GstRate {
        category.rate_for_supply(is_inter_state.unwrap_or(self.default_is_inter_state))
    }

    /// Set a custom GST rate for a specific product/service
    ///
    /// # Errors
    ///
    /// Any error from [`GstRate::validate`].
    pub fn set_custom_rate(
        &mut self,
        product_code: impl Into<String>,
        gst_rate: GstRate,
    ) -> Result<(), GstError> {
        gst_rate.validate()?;
        self.custom_rates.insert(product_code.into(), gst_rate);
        Ok(())
    }

    /// Calculate GST for a product using category rates
    ///
    /// # Errors
    ///
    /// Any error from [`GstCalculation::calculate`].
    pub fn calculate_by_category(
        &self,
        base_amount: BigDecimal,
        category: GstCategory,
        is_inter_state: Option<bool>,
    ) -> Result<GstCalculation, GstError> {
        GstCalculation::calculate(base_amount, self.category_rate(category, is_inter_state))
    }

    /// Calculate GST for a product using custom rates
    ///
    /// # Errors
    ///
    /// [`GstError::ProductNotFound`] when no custom rate is set for `product_code`.
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

    /// Reverse calculate base amount from total
    ///
    /// # Errors
    ///
    /// Any error from [`GstCalculation::reverse_calculate`].
    pub fn reverse_calculate_by_category(
        &self,
        total_amount: BigDecimal,
        category: GstCategory,
        is_inter_state: Option<bool>,
    ) -> Result<GstCalculation, GstError> {
        GstCalculation::reverse_calculate(
            total_amount,
            self.category_rate(category, is_inter_state),
        )
    }
}

/// GST-related errors
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GstError {
    /// CGST + SGST + IGST differs from the total rate
    #[error("GST components add up to {components}, not the total rate {total}")]
    ComponentsMismatch {
        /// Sum of the components
        components: BigDecimal,
        /// Declared total rate
        total: BigDecimal,
    },
    /// CGST and SGST differ on an intra-state rate
    #[error("CGST ({cgst}) and SGST ({sgst}) must be equal for intra-state supply")]
    UnequalSplit {
        /// CGST rate
        cgst: BigDecimal,
        /// SGST rate
        sgst: BigDecimal,
    },
    /// IGST is charged together with CGST or SGST
    #[error("only IGST applies to inter-state supply")]
    MixedIgstAndSplit,
    /// No custom rate is set for this product code
    #[error("product not found: {0}")]
    ProductNotFound(String),
}
