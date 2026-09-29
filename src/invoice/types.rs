//! GST invoice domain types

use super::hsn_lookup::{is_valid_hsn_sac, HsnMaster};
use crate::tax::gst::{GstCalculation, GstError, GstRate};
use bigdecimal::{BigDecimal, Signed};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Characters a GSTIN is built from, in the order the checksum algorithm assigns them values
const GSTIN_CHARSET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Length of a GSTIN
const GSTIN_LEN: usize = 15;

/// Maximum length of an invoice number (Rule 46 of the CGST Rules)
const INVOICE_NUMBER_MAX_LEN: usize = 16;

/// Largest GST rate a line item may carry, as a percentage
const MAX_GST_RATE: u32 = 100;

/// A validated GST Identification Number
///
/// A GSTIN is 15 characters, laid out as:
///
/// | Position | Meaning                                          |
/// |----------|--------------------------------------------------|
/// | 1-2      | State code (`01`-`38`, `97` or `99`)             |
/// | 3-12     | PAN of the taxpayer (`AAAAA9999A`)               |
/// | 13       | Entity number for the same PAN (`1`-`9`, `A`-`Z`) |
/// | 14       | `Z` by default                                   |
/// | 15       | Mod-36 checksum over the first 14 characters     |
///
/// Lowercase input is accepted and normalised to uppercase. Deserialising goes through the same
/// validation, so a `Gstin` value is always well-formed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Gstin(String);

impl Gstin {
    /// Parse and validate a GSTIN
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidGstin`] naming the first rule the value breaks.
    pub fn parse(value: &str) -> Result<Self, InvoiceError> {
        let gstin = value.to_ascii_uppercase();
        check_gstin(gstin.as_bytes()).map_err(|reason| InvoiceError::InvalidGstin {
            value: value.to_string(),
            reason,
        })?;
        Ok(Self(gstin))
    }

    /// The GSTIN as a string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two-digit state code the taxpayer is registered in
    #[must_use]
    pub fn state_code(&self) -> &str {
        &self.0[0..2]
    }

    /// PAN embedded in the GSTIN
    #[must_use]
    pub fn pan(&self) -> &str {
        &self.0[2..12]
    }
}

/// Why a GSTIN was rejected
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GstinError {
    /// Not exactly 15 characters
    #[error("must be exactly {GSTIN_LEN} characters")]
    Length,
    /// The first two characters are not a known state code
    #[error("invalid state code")]
    StateCode,
    /// Characters 3-12 are not shaped like a PAN
    #[error("characters 3-12 must be a PAN (AAAAA9999A)")]
    Pan,
    /// Character 13 is not 1-9 or A-Z
    #[error("entity number must be 1-9 or A-Z")]
    EntityNumber,
    /// Character 14 is not `Z`
    #[error("character 14 must be 'Z'")]
    DefaultZ,
    /// The check character does not match
    #[error("checksum mismatch, expected '{expected}'")]
    Checksum {
        /// The check character the first 14 characters call for
        expected: char,
    },
}

/// Run every GSTIN rule over an uppercased value
fn check_gstin(bytes: &[u8]) -> Result<(), GstinError> {
    let (Some(body), Some(&check), GSTIN_LEN) = (bytes.get(..14), bytes.get(14), bytes.len())
    else {
        return Err(GstinError::Length);
    };

    check_rule(is_valid_state_code(&body[0..2]), GstinError::StateCode)?;
    check_rule(is_pan(&body[2..12]), GstinError::Pan)?;
    check_rule(
        matches!(body[12], b'1'..=b'9') || body[12].is_ascii_uppercase(),
        GstinError::EntityNumber,
    )?;
    check_rule(body[13] == b'Z', GstinError::DefaultZ)?;
    match gstin_checksum(body) {
        Some(expected) if expected == check => Ok(()),
        Some(expected) => Err(GstinError::Checksum {
            expected: char::from(expected),
        }),
        // Unreachable after the checks above, which admit only charset characters
        None => Err(GstinError::Pan),
    }
}

fn check_rule(holds: bool, error: GstinError) -> Result<(), GstinError> {
    if holds {
        Ok(())
    } else {
        Err(error)
    }
}

/// State and union territory codes, plus `97` (other territory) and `99` (centre jurisdiction)
fn is_valid_state_code(code: &[u8]) -> bool {
    code.iter().all(u8::is_ascii_digit)
        && matches!(
            code.iter().fold(0u32, |n, d| n * 10 + u32::from(d - b'0')),
            1..=38 | 97 | 99
        )
}

/// Whether ten characters have the shape of a PAN: five letters, four digits, one letter
fn is_pan(pan: &[u8]) -> bool {
    pan.len() == 10
        && pan[0..5].iter().all(u8::is_ascii_uppercase)
        && pan[5..9].iter().all(u8::is_ascii_digit)
        && pan[9].is_ascii_uppercase()
}

/// Compute the GSTIN check character for the first 14 characters
///
/// Returns `None` if any character is outside [`GSTIN_CHARSET`].
pub(super) fn gstin_checksum(body: &[u8]) -> Option<u8> {
    let radix = GSTIN_CHARSET.len();
    let sum = body
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let value = GSTIN_CHARSET.iter().position(|x| x == c)?;
            let product = value * if i % 2 == 0 { 1 } else { 2 };
            Some(product / radix + product % radix)
        })
        .sum::<Option<usize>>()?;

    GSTIN_CHARSET.get((radix - sum % radix) % radix).copied()
}

impl FromStr for Gstin {
    type Err = InvoiceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Gstin {
    type Error = InvoiceError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Gstin> for String {
    fn from(gstin: Gstin) -> Self {
        gstin.0
    }
}

impl fmt::Display for Gstin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tax components for an amount, or the sum of them across an invoice
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GstBreakdown {
    /// Taxable value (before GST)
    pub taxable_value: BigDecimal,
    /// Central GST amount
    pub cgst: BigDecimal,
    /// State GST amount
    pub sgst: BigDecimal,
    /// Integrated GST amount
    pub igst: BigDecimal,
    /// Total tax (CGST + SGST + IGST)
    pub total_tax: BigDecimal,
    /// Taxable value plus total tax
    pub total: BigDecimal,
}

impl From<GstCalculation> for GstBreakdown {
    fn from(calculation: GstCalculation) -> Self {
        Self {
            taxable_value: calculation.base_amount,
            cgst: calculation.cgst_amount,
            sgst: calculation.sgst_amount,
            igst: calculation.igst_amount,
            total_tax: calculation.total_gst_amount,
            total: calculation.total_amount,
        }
    }
}

impl<'a> std::iter::Sum<&'a GstBreakdown> for GstBreakdown {
    fn sum<I: Iterator<Item = &'a GstBreakdown>>(iter: I) -> Self {
        iter.fold(Self::default(), |mut total, line| {
            total += line;
            total
        })
    }
}

impl std::ops::AddAssign<&GstBreakdown> for GstBreakdown {
    fn add_assign(&mut self, other: &GstBreakdown) {
        self.taxable_value += &other.taxable_value;
        self.cgst += &other.cgst;
        self.sgst += &other.sgst;
        self.igst += &other.igst;
        self.total_tax += &other.total_tax;
        self.total += &other.total;
    }
}

/// A line on a GST invoice
///
/// Deserialising goes through [`GstLineItem::new`], so the same rules apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGstLineItem")]
pub struct GstLineItem {
    /// HSN code (goods) or SAC code (services): 4, 6 or 8 digits
    pub hsn_sac: String,
    /// Item description
    pub description: String,
    /// Quantity supplied
    pub quantity: BigDecimal,
    /// Unit price before GST
    pub unit_price: BigDecimal,
    /// Total GST rate as a percentage (e.g. 18 for 18%)
    pub gst_rate: BigDecimal,
}

/// Unvalidated shape of a [`GstLineItem`], used when deserialising
#[derive(Deserialize)]
struct RawGstLineItem {
    hsn_sac: String,
    description: String,
    quantity: BigDecimal,
    unit_price: BigDecimal,
    gst_rate: BigDecimal,
}

impl TryFrom<RawGstLineItem> for GstLineItem {
    type Error = InvoiceError;

    fn try_from(raw: RawGstLineItem) -> Result<Self, Self::Error> {
        Self::new(
            raw.hsn_sac,
            raw.description,
            raw.quantity,
            raw.unit_price,
            raw.gst_rate,
        )
    }
}

impl GstLineItem {
    /// Create a validated line item
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidHsnSac`] for a malformed code, or [`InvoiceError::InvalidLineItem`]
    /// naming the first rule the other fields break.
    pub fn new(
        hsn_sac: impl Into<String>,
        description: impl Into<String>,
        quantity: BigDecimal,
        unit_price: BigDecimal,
        gst_rate: BigDecimal,
    ) -> Result<Self, InvoiceError> {
        let hsn_sac = hsn_sac.into();
        if !is_valid_hsn_sac(&hsn_sac) {
            return Err(InvoiceError::InvalidHsnSac(hsn_sac));
        }

        let item = Self {
            hsn_sac,
            description: description.into(),
            quantity,
            unit_price,
            gst_rate,
        };
        item.check().map_err(InvoiceError::InvalidLineItem)?;
        Ok(item)
    }

    /// The field rules other than the HSN/SAC shape
    pub(super) fn check(&self) -> Result<(), LineItemError> {
        if self.description.trim().is_empty() {
            Err(LineItemError::EmptyDescription)
        } else if !self.quantity.is_positive() {
            Err(LineItemError::NonPositiveQuantity)
        } else if self.unit_price.is_negative() {
            Err(LineItemError::NegativeUnitPrice)
        } else if self.gst_rate.is_negative() || self.gst_rate > MAX_GST_RATE {
            Err(LineItemError::RateOutOfRange(self.gst_rate.clone()))
        } else {
            Ok(())
        }
    }

    /// Create a validated line item at the default GST rate for its HSN/SAC code
    ///
    /// The rate comes from [`HsnMaster::global`], falling back from the exact code to its 6- and
    /// 4-digit headings. Use [`GstLineItem::new`] to charge a different rate.
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidHsnSac`] for a malformed code, [`InvoiceError::UnknownHsnSac`] for a
    /// code the master does not know, or any error from [`GstLineItem::new`].
    pub fn with_default_rate(
        hsn_sac: impl Into<String>,
        description: impl Into<String>,
        quantity: BigDecimal,
        unit_price: BigDecimal,
    ) -> Result<Self, InvoiceError> {
        let hsn_sac = hsn_sac.into();
        let Some(gst_rate) = HsnMaster::global().default_rate(&hsn_sac) else {
            return Err(if is_valid_hsn_sac(&hsn_sac) {
                InvoiceError::UnknownHsnSac(hsn_sac)
            } else {
                InvoiceError::InvalidHsnSac(hsn_sac)
            });
        };

        Self::new(hsn_sac, description, quantity, unit_price, gst_rate)
    }

    /// Taxable value of the line (quantity x unit price), exactly
    ///
    /// This is not rounded; [`GstLineItem::breakdown`] rounds it to paise, so its
    /// `taxable_value` is the amount that is invoiced and posted.
    #[must_use]
    pub fn taxable_value(&self) -> BigDecimal {
        &self.quantity * &self.unit_price
    }

    /// Tax breakdown for this line, split as IGST for inter-state supply or CGST + SGST otherwise
    ///
    /// # Errors
    ///
    /// [`InvoiceError::Gst`] if the rate fails [`GstRate::validate`].
    pub fn breakdown(&self, is_inter_state: bool) -> Result<GstBreakdown, InvoiceError> {
        let rate = GstRate::for_supply(self.gst_rate.clone(), is_inter_state);
        Ok(GstCalculation::calculate(self.taxable_value(), rate)?.into())
    }
}

/// A B2B tax invoice under Indian GST
///
/// Whether the supply is inter-state (IGST) or intra-state (CGST + SGST) is derived from the
/// state codes of the seller and buyer GSTINs. Deserialising goes through [`GstInvoice::new`],
/// so the same rules apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGstInvoice")]
pub struct GstInvoice {
    /// Invoice number: at most 16 characters of letters, digits, `-` and `/`
    pub invoice_number: String,
    /// Date of issue
    pub invoice_date: NaiveDate,
    /// Supplier's GSTIN
    pub seller_gstin: Gstin,
    /// Recipient's GSTIN
    pub buyer_gstin: Gstin,
    /// Invoice lines
    pub line_items: Vec<GstLineItem>,
}

/// Unvalidated shape of a [`GstInvoice`], used when deserialising
#[derive(Deserialize)]
struct RawGstInvoice {
    invoice_number: String,
    invoice_date: NaiveDate,
    seller_gstin: Gstin,
    buyer_gstin: Gstin,
    line_items: Vec<GstLineItem>,
}

impl TryFrom<RawGstInvoice> for GstInvoice {
    type Error = InvoiceError;

    fn try_from(raw: RawGstInvoice) -> Result<Self, Self::Error> {
        Self::new(
            raw.invoice_number,
            raw.invoice_date,
            raw.seller_gstin,
            raw.buyer_gstin,
            raw.line_items,
        )
    }
}

impl GstInvoice {
    /// Create a validated invoice
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidInvoiceNumber`] or [`InvoiceError::EmptyInvoice`].
    pub fn new(
        invoice_number: impl Into<String>,
        invoice_date: NaiveDate,
        seller_gstin: Gstin,
        buyer_gstin: Gstin,
        line_items: Vec<GstLineItem>,
    ) -> Result<Self, InvoiceError> {
        let invoice_number = invoice_number.into();
        validate_invoice_number(&invoice_number)?;

        if line_items.is_empty() {
            return Err(InvoiceError::EmptyInvoice);
        }

        Ok(Self {
            invoice_number,
            invoice_date,
            seller_gstin,
            buyer_gstin,
            line_items,
        })
    }

    /// Whether seller and buyer are registered in different states
    #[must_use]
    pub fn is_inter_state(&self) -> bool {
        self.seller_gstin.state_code() != self.buyer_gstin.state_code()
    }

    /// Tax breakdown for each line, in order
    ///
    /// # Errors
    ///
    /// The first error from [`GstLineItem::breakdown`].
    pub fn line_breakdowns(&self) -> Result<Vec<GstBreakdown>, InvoiceError> {
        let is_inter_state = self.is_inter_state();
        self.line_items
            .iter()
            .map(|item| item.breakdown(is_inter_state))
            .collect()
    }

    /// Tax breakdown summed across all lines
    ///
    /// # Errors
    ///
    /// The first error from [`GstLineItem::breakdown`].
    pub fn breakdown(&self) -> Result<GstBreakdown, InvoiceError> {
        Ok(self.line_breakdowns()?.iter().sum())
    }
}

/// Check an invoice number against Rule 46: unique per financial year (not checked here), at
/// most 16 characters, and only letters, digits, `-` and `/`
pub(super) fn validate_invoice_number(invoice_number: &str) -> Result<(), InvoiceError> {
    invoice_number_error(invoice_number).map_or(Ok(()), |reason| {
        Err(InvoiceError::InvalidInvoiceNumber {
            value: invoice_number.to_string(),
            reason,
        })
    })
}

/// The first Rule 46 check an invoice number fails, if any
pub(super) fn invoice_number_error(invoice_number: &str) -> Option<InvoiceNumberError> {
    if invoice_number.is_empty() || invoice_number.len() > INVOICE_NUMBER_MAX_LEN {
        Some(InvoiceNumberError::Length)
    } else if !invoice_number
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
    {
        Some(InvoiceNumberError::Characters)
    } else {
        None
    }
}

/// Why an invoice number was rejected
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvoiceNumberError {
    /// Empty or longer than 16 characters
    #[error("must be 1-{INVOICE_NUMBER_MAX_LEN} characters")]
    Length,
    /// Contains something other than letters, digits, `-` and `/`
    #[error("only letters, digits, '-' and '/' are allowed")]
    Characters,
}

/// Why a line item was rejected
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LineItemError {
    /// The description is empty or only whitespace
    #[error("description cannot be empty")]
    EmptyDescription,
    /// The quantity is zero or negative
    #[error("quantity must be positive")]
    NonPositiveQuantity,
    /// The unit price is negative
    #[error("unit price cannot be negative")]
    NegativeUnitPrice,
    /// The GST rate is outside 0-100
    #[error("GST rate must be between 0 and {MAX_GST_RATE}, got {0}")]
    RateOutOfRange(BigDecimal),
}

/// Invoice-related errors
#[derive(Debug, thiserror::Error)]
pub enum InvoiceError {
    /// The GSTIN is malformed
    #[error("invalid GSTIN {value}: {reason}")]
    InvalidGstin {
        /// The value as given
        value: String,
        /// The first rule it breaks
        reason: GstinError,
    },
    /// The invoice number breaks Rule 46
    #[error("invalid invoice number {value}: {reason}")]
    InvalidInvoiceNumber {
        /// The value as given
        value: String,
        /// The first rule it breaks
        reason: InvoiceNumberError,
    },
    /// The HSN/SAC code is not 4, 6 or 8 digits
    #[error("invalid HSN/SAC code: {0}")]
    InvalidHsnSac(String),
    /// The HSN/SAC code is well-formed but not in the master data
    #[error("HSN/SAC code not in the master data: {0}")]
    UnknownHsnSac(String),
    /// A line item field is invalid
    #[error("invalid line item: {0}")]
    InvalidLineItem(LineItemError),
    /// The invoice has no line items
    #[error("an invoice needs at least one line item")]
    EmptyInvoice,
    /// The GST rate is inconsistent
    #[error(transparent)]
    Gst(#[from] GstError),
}
