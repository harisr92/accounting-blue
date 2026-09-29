//! GST (Goods and Services Tax) calculation engine for Indian tax compliance

use bigdecimal::{BigDecimal, RoundingMode, Signed, Zero};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Decimal places in an amount of rupees: one paisa is 0.01
pub const PAISE_SCALE: i64 = 2;

/// `rate` percent of `amount`, exactly; [`GstCalculation`] rounds the result to paise
#[must_use]
pub fn percent_of(amount: &BigDecimal, rate: &BigDecimal) -> BigDecimal {
    (amount * rate) / BigDecimal::from(100)
}

/// Round an amount of rupees to the nearest paisa, with halves rounded away from zero
///
/// This is the one rounding rule for money in the crate: 0.025 becomes 0.03 and 0.0249
/// becomes 0.02. The result always has [`PAISE_SCALE`] decimal places.
///
/// # Example
///
/// ```
/// use accounting_core::tax::round_to_paise;
/// use bigdecimal::BigDecimal;
/// use std::str::FromStr;
///
/// let round = |s| round_to_paise(&BigDecimal::from_str(s).unwrap()).to_string();
/// assert_eq!(round("0.02475"), "0.02");
/// assert_eq!(round("0.025"), "0.03");
/// assert_eq!(round("-0.025"), "-0.03");
/// assert_eq!(round("90"), "90.00");
/// ```
#[must_use]
pub fn round_to_paise(amount: &BigDecimal) -> BigDecimal {
    amount.with_scale_round(PAISE_SCALE, RoundingMode::HalfUp)
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
    /// Every amount in the result is in paise: the base is rounded with [`round_to_paise`],
    /// each of CGST, SGST and IGST is rounded on its own, the total tax is the sum of the
    /// rounded components and the total is the base plus that tax. CGST and SGST therefore stay
    /// equal on an intra-state rate.
    ///
    /// # Errors
    ///
    /// Any error from [`GstRate::validate`].
    #[allow(clippy::needless_pass_by_value)] // takes ownership; rounding builds a new value
    pub fn calculate(base_amount: BigDecimal, gst_rate: GstRate) -> Result<Self, GstError> {
        gst_rate.validate()?;

        let base_amount = round_to_paise(&base_amount);
        let component = |rate: &BigDecimal| round_to_paise(&percent_of(&base_amount, rate));
        let cgst_amount = component(&gst_rate.cgst_rate);
        let sgst_amount = component(&gst_rate.sgst_rate);
        let igst_amount = component(&gst_rate.igst_rate);

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
    /// The total is rounded to paise and kept exactly: the result's `total_amount` is the given
    /// total, and its base plus its tax always add up to it, so the result can be posted against
    /// the amount actually received. The tax is calculated with [`GstCalculation::calculate`]
    /// on `total * 100 / (100 + rate)`, so CGST and SGST stay equal, and the base absorbs the
    /// rounding: it is the total minus the tax. At any rate up to 40% (every GST slab) the base
    /// is within a paisa of the exact division. At higher intra-state rates a total of a few
    /// paise can leave it further off: at 100%, a total of 0.01 gives a base of -0.01.
    ///
    /// # Errors
    ///
    /// Any error from [`GstRate::validate`].
    #[allow(clippy::needless_pass_by_value)] // takes ownership to mirror `calculate`
    pub fn reverse_calculate(
        total_amount: BigDecimal,
        gst_rate: GstRate,
    ) -> Result<Self, GstError> {
        let total_amount = round_to_paise(&total_amount);
        let divisor = BigDecimal::from(100) + &gst_rate.total_rate;
        let estimated_base = (&total_amount * BigDecimal::from(100)) / divisor;
        let forward = Self::calculate(estimated_base, gst_rate)?;

        Ok(Self {
            base_amount: &total_amount - &forward.total_gst_amount,
            total_amount,
            ..forward
        })
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
