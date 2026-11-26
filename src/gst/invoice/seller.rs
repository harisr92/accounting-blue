//! Who issues a GST document: the seller's registration
//!
//! A [`SellerRegistration`] says whether the seller may charge GST. A regular taxpayer issues tax
//! invoices. A composition taxable person (Section 10 of the CGST Act) and a seller not registered
//! under GST may not collect tax, so what they issue is a bill of supply (Rule 49 of the CGST
//! Rules): no tax on any line, and, for a composition seller, the declaration Rule 5(1)(g) asks
//! for. A composition seller still pays tax on its own turnover, at its [`CompositionRate`].

use super::types::{Gstin, InvoiceError, StateCode};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Composition rate, in percent, for traders and manufacturers (Rule 7 of the CGST Rules)
pub const TRADER_OR_MANUFACTURER_PERCENT: u32 = 1;

/// Composition rate, in percent, for restaurants not serving alcohol (Rule 7)
pub const RESTAURANT_PERCENT: u32 = 5;

/// Composition rate, in percent, for suppliers of services (Notification 2/2019-Central Tax
/// (Rate))
pub const SERVICES_PERCENT: u32 = 6;

/// Declaration a composition taxable person prints at the top of each bill of supply (Rule
/// 5(1)(g) of the CGST Rules)
pub const COMPOSITION_DECLARATION: &str =
    "Composition taxable person, not eligible to collect tax on supplies";

/// Declaration printed on a bill of supply from a seller not registered under GST
pub const UNREGISTERED_DECLARATION: &str = "Supplier not registered under GST; no tax charged";

/// Rate at which a composition taxable person pays tax on its turnover, by its kind of business
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionRate {
    /// A trader or manufacturer: [`TRADER_OR_MANUFACTURER_PERCENT`]
    TraderOrManufacturer,
    /// A restaurant not serving alcohol: [`RESTAURANT_PERCENT`]
    Restaurant,
    /// A supplier of services: [`SERVICES_PERCENT`]
    Services,
}

impl CompositionRate {
    /// Every composition rate, lowest first
    pub const ALL: [Self; 3] = [Self::TraderOrManufacturer, Self::Restaurant, Self::Services];

    /// The rate in percent, e.g. `1` for a trader
    #[must_use]
    pub fn percent(self) -> BigDecimal {
        BigDecimal::from(match self {
            Self::TraderOrManufacturer => TRADER_OR_MANUFACTURER_PERCENT,
            Self::Restaurant => RESTAURANT_PERCENT,
            Self::Services => SERVICES_PERCENT,
        })
    }
}

impl TryFrom<&BigDecimal> for CompositionRate {
    type Error = InvoiceError;

    /// The composition rate of `percent`
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidCompositionRate`] for a rate that is not one of the scheme's.
    fn try_from(percent: &BigDecimal) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|rate| rate.percent() == *percent)
            .ok_or_else(|| InvoiceError::InvalidCompositionRate(percent.clone()))
    }
}

impl fmt::Display for CompositionRate {
    /// The rate, e.g. `1%`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.percent())
    }
}

/// The seller's registration under GST, which decides whether its documents may charge tax
///
/// # Example
///
/// ```
/// use accounting_core::gst::invoice::{CompositionRate, Gstin, SellerRegistration, StateCode};
///
/// # fn main() -> Result<(), accounting_core::gst::invoice::InvoiceError> {
/// let regular = SellerRegistration::from(Gstin::parse("27AAPFU0939F1ZV")?);
/// assert!(regular.charges_tax());
///
/// let composition = SellerRegistration::Composition {
///     gstin: Gstin::parse("27AAPFU0939F1ZV")?,
///     rate: CompositionRate::TraderOrManufacturer,
/// };
/// assert!(!composition.charges_tax());
/// assert!(composition.declaration().is_some());
///
/// let unregistered = SellerRegistration::Unregistered { state: StateCode::parse("29")? };
/// assert_eq!(unregistered.gstin(), None);
/// assert_eq!(unregistered.state(), "29");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SellerRegistration {
    /// A regular taxpayer, who charges GST on its tax invoices
    Regular(Gstin),
    /// A composition taxable person (Section 10), who charges no GST and pays tax on its
    /// turnover at `rate`
    Composition {
        /// The seller's GSTIN
        gstin: Gstin,
        /// Rate of tax on its turnover
        rate: CompositionRate,
    },
    /// A seller not registered under GST, who charges no GST
    Unregistered {
        /// State the seller supplies from
        state: StateCode,
    },
}

impl SellerRegistration {
    /// The seller's GSTIN; `None` when it is not registered
    #[must_use]
    pub fn gstin(&self) -> Option<&Gstin> {
        match self {
            Self::Regular(gstin) | Self::Composition { gstin, .. } => Some(gstin),
            Self::Unregistered { .. } => None,
        }
    }

    /// State code of the seller: its GSTIN's first two digits, or the state it supplies from
    #[must_use]
    pub fn state(&self) -> &str {
        match self {
            Self::Regular(gstin) | Self::Composition { gstin, .. } => gstin.state_code(),
            Self::Unregistered { state } => state.as_str(),
        }
    }

    /// Whether the seller charges GST: only a regular taxpayer does
    #[must_use]
    pub fn charges_tax(&self) -> bool {
        matches!(self, Self::Regular(_))
    }

    /// The composition rate; `None` unless the seller is under composition
    #[must_use]
    pub fn composition_rate(&self) -> Option<CompositionRate> {
        match self {
            Self::Composition { rate, .. } => Some(*rate),
            Self::Regular(_) | Self::Unregistered { .. } => None,
        }
    }

    /// The declaration the seller's bills of supply print: [`COMPOSITION_DECLARATION`] or
    /// [`UNREGISTERED_DECLARATION`]; `None` for a regular taxpayer
    #[must_use]
    pub fn declaration(&self) -> Option<&'static str> {
        match self {
            Self::Regular(_) => None,
            Self::Composition { .. } => Some(COMPOSITION_DECLARATION),
            Self::Unregistered { .. } => Some(UNREGISTERED_DECLARATION),
        }
    }
}

impl From<Gstin> for SellerRegistration {
    /// A regular taxpayer with this GSTIN
    fn from(gstin: Gstin) -> Self {
        Self::Regular(gstin)
    }
}

impl fmt::Display for SellerRegistration {
    /// The GSTIN, with `(composition)` for a composition seller, or `Unregistered (state NN)`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular(gstin) => gstin.fmt(f),
            Self::Composition { gstin, .. } => write!(f, "{gstin} (composition)"),
            Self::Unregistered { state } => write!(f, "Unregistered (state {state})"),
        }
    }
}

/// How a seller is read from JSON: a bare GSTIN, as documents saved before
/// [`SellerRegistration`] existed hold it, or a registration
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawSeller {
    /// A regular taxpayer's GSTIN, as `seller_gstin` held it
    Gstin(Gstin),
    /// A registration
    Registration(SellerRegistration),
}

impl From<RawSeller> for SellerRegistration {
    fn from(raw: RawSeller) -> Self {
        match raw {
            RawSeller::Gstin(gstin) => Self::Regular(gstin),
            RawSeller::Registration(registration) => registration,
        }
    }
}
