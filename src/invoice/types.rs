//! GST invoice domain types

use super::hsn_lookup::{is_valid_hsn_sac, HsnMaster};
use crate::tax::gst::{GstCalculation, GstError, GstRate};
use bigdecimal::BigDecimal;
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
    pub fn parse(value: &str) -> Result<Self, InvoiceError> {
        let gstin = value.to_ascii_uppercase();
        let bytes = gstin.as_bytes();

        if bytes.len() != GSTIN_LEN {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: must be exactly {GSTIN_LEN} characters"
            )));
        }

        let state_code = &gstin[0..2];
        if !is_valid_state_code(state_code) {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: invalid state code '{state_code}'"
            )));
        }

        let pan = &bytes[2..12];
        let pan_is_valid = pan[0..5].iter().all(u8::is_ascii_uppercase)
            && pan[5..9].iter().all(u8::is_ascii_digit)
            && pan[9].is_ascii_uppercase();
        if !pan_is_valid {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: characters 3-12 must be a PAN (AAAAA9999A)"
            )));
        }

        let entity = bytes[12];
        if !(matches!(entity, b'1'..=b'9') || entity.is_ascii_uppercase()) {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: entity number must be 1-9 or A-Z"
            )));
        }

        if bytes[13] != b'Z' {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: character 14 must be 'Z'"
            )));
        }

        let expected = gstin_checksum(&bytes[..14]);
        if bytes[14] != expected {
            return Err(InvoiceError::InvalidGstin(format!(
                "{value}: checksum mismatch, expected '{}'",
                expected as char
            )));
        }

        Ok(Self(gstin))
    }

    /// The GSTIN as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two-digit state code the taxpayer is registered in
    pub fn state_code(&self) -> &str {
        &self.0[0..2]
    }

    /// PAN embedded in the GSTIN
    pub fn pan(&self) -> &str {
        &self.0[2..12]
    }
}

/// State and union territory codes, plus `97` (other territory) and `99` (centre jurisdiction)
fn is_valid_state_code(code: &str) -> bool {
    matches!(code.parse::<u8>(), Ok(1..=38 | 97 | 99))
}

/// Compute the GSTIN check character for the first 14 characters
///
/// Callers must pass characters from [`GSTIN_CHARSET`] only.
fn gstin_checksum(body: &[u8]) -> u8 {
    let sum: u32 = body
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let value = GSTIN_CHARSET.iter().position(|x| x == c).unwrap_or(0) as u32;
            let product = value * if i % 2 == 0 { 1 } else { 2 };
            product / 36 + product % 36
        })
        .sum();

    GSTIN_CHARSET[((36 - sum % 36) % 36) as usize]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl GstBreakdown {
    fn zero() -> Self {
        Self {
            taxable_value: crate::ZERO.clone(),
            cgst: crate::ZERO.clone(),
            sgst: crate::ZERO.clone(),
            igst: crate::ZERO.clone(),
            total_tax: crate::ZERO.clone(),
            total: crate::ZERO.clone(),
        }
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl GstLineItem {
    /// Create a validated line item
    pub fn new(
        hsn_sac: String,
        description: String,
        quantity: BigDecimal,
        unit_price: BigDecimal,
        gst_rate: BigDecimal,
    ) -> Result<Self, InvoiceError> {
        if !is_valid_hsn_sac(&hsn_sac) {
            return Err(InvoiceError::InvalidHsnSac(hsn_sac));
        }

        if description.trim().is_empty() {
            return Err(InvoiceError::InvalidLineItem(
                "description cannot be empty".to_string(),
            ));
        }

        if quantity <= *crate::ZERO {
            return Err(InvoiceError::InvalidLineItem(
                "quantity must be positive".to_string(),
            ));
        }

        if unit_price < *crate::ZERO {
            return Err(InvoiceError::InvalidLineItem(
                "unit price cannot be negative".to_string(),
            ));
        }

        if gst_rate < *crate::ZERO || gst_rate > 100 {
            return Err(InvoiceError::InvalidLineItem(format!(
                "GST rate must be between 0 and 100, got {gst_rate}"
            )));
        }

        Ok(Self {
            hsn_sac,
            description,
            quantity,
            unit_price,
            gst_rate,
        })
    }

    /// Create a validated line item at the default GST rate for its HSN/SAC code
    ///
    /// The rate comes from [`HsnMaster::global`], falling back from the exact code to its 6- and
    /// 4-digit headings. Use [`GstLineItem::new`] to charge a different rate.
    pub fn with_default_rate(
        hsn_sac: String,
        description: String,
        quantity: BigDecimal,
        unit_price: BigDecimal,
    ) -> Result<Self, InvoiceError> {
        if !is_valid_hsn_sac(&hsn_sac) {
            return Err(InvoiceError::InvalidHsnSac(hsn_sac));
        }

        let gst_rate = HsnMaster::global()
            .default_rate(&hsn_sac)
            .ok_or_else(|| InvoiceError::UnknownHsnSac(hsn_sac.clone()))?;

        Self::new(hsn_sac, description, quantity, unit_price, gst_rate)
    }

    /// Taxable value of the line (quantity x unit price)
    pub fn taxable_value(&self) -> BigDecimal {
        &self.quantity * &self.unit_price
    }

    /// Tax breakdown for this line, split as IGST for inter-state supply or CGST + SGST otherwise
    pub fn breakdown(&self, is_inter_state: bool) -> Result<GstBreakdown, InvoiceError> {
        let rate = if is_inter_state {
            GstRate::inter_state(self.gst_rate.clone())
        } else {
            GstRate::intra_state(self.gst_rate.clone())
        };

        Ok(GstCalculation::calculate(self.taxable_value(), rate)?.into())
    }
}

/// A B2B tax invoice under Indian GST
///
/// Whether the supply is inter-state (IGST) or intra-state (CGST + SGST) is derived from the
/// state codes of the seller and buyer GSTINs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl GstInvoice {
    /// Create a validated invoice
    pub fn new(
        invoice_number: String,
        invoice_date: NaiveDate,
        seller_gstin: Gstin,
        buyer_gstin: Gstin,
        line_items: Vec<GstLineItem>,
    ) -> Result<Self, InvoiceError> {
        validate_invoice_number(&invoice_number)?;

        if line_items.is_empty() {
            return Err(InvoiceError::InvalidLineItem(
                "an invoice needs at least one line item".to_string(),
            ));
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
    pub fn is_inter_state(&self) -> bool {
        self.seller_gstin.state_code() != self.buyer_gstin.state_code()
    }

    /// Tax breakdown for each line, in order
    pub fn line_breakdowns(&self) -> Result<Vec<GstBreakdown>, InvoiceError> {
        let is_inter_state = self.is_inter_state();
        self.line_items
            .iter()
            .map(|item| item.breakdown(is_inter_state))
            .collect()
    }

    /// Tax breakdown summed across all lines
    pub fn breakdown(&self) -> Result<GstBreakdown, InvoiceError> {
        let mut total = GstBreakdown::zero();
        for line in self.line_breakdowns()? {
            total += &line;
        }
        Ok(total)
    }
}

/// Check an invoice number against Rule 46: unique per financial year (not checked here), at
/// most 16 characters, and only letters, digits, `-` and `/`
fn validate_invoice_number(invoice_number: &str) -> Result<(), InvoiceError> {
    if invoice_number.is_empty() || invoice_number.len() > INVOICE_NUMBER_MAX_LEN {
        return Err(InvoiceError::InvalidInvoiceNumber(format!(
            "{invoice_number}: must be 1-{INVOICE_NUMBER_MAX_LEN} characters"
        )));
    }

    if !invoice_number
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
    {
        return Err(InvoiceError::InvalidInvoiceNumber(format!(
            "{invoice_number}: only letters, digits, '-' and '/' are allowed"
        )));
    }

    Ok(())
}

/// Invoice-related errors
#[derive(Debug, thiserror::Error)]
pub enum InvoiceError {
    #[error("Invalid GSTIN: {0}")]
    InvalidGstin(String),
    #[error("Invalid invoice number: {0}")]
    InvalidInvoiceNumber(String),
    #[error("Invalid HSN/SAC code: {0}")]
    InvalidHsnSac(String),
    #[error("HSN/SAC code not in the master data: {0}")]
    UnknownHsnSac(String),
    #[error("Invalid line item: {0}")]
    InvalidLineItem(String),
    #[error(transparent)]
    Gst(#[from] GstError),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELLER: &str = "27AAPFU0939F1ZV";

    fn gstin_with_checksum(body: &str) -> String {
        format!("{body}{}", gstin_checksum(body.as_bytes()) as char)
    }

    fn item(rate: i32) -> GstLineItem {
        GstLineItem::new(
            "998314".to_string(),
            "IT consulting".to_string(),
            BigDecimal::from(2),
            BigDecimal::from(500),
            BigDecimal::from(rate),
        )
        .unwrap()
    }

    fn invoice(buyer: &str, line_items: Vec<GstLineItem>) -> GstInvoice {
        GstInvoice::new(
            "INV/2024-25/001".to_string(),
            NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
            Gstin::parse(SELLER).unwrap(),
            Gstin::parse(buyer).unwrap(),
            line_items,
        )
        .unwrap()
    }

    #[test]
    fn test_gstin_valid() {
        let gstin = Gstin::parse(SELLER).unwrap();
        assert_eq!(gstin.state_code(), "27");
        assert_eq!(gstin.pan(), "AAPFU0939F");
        assert_eq!(gstin.to_string(), SELLER);
    }

    #[test]
    fn test_gstin_normalises_case() {
        let gstin = Gstin::parse(&SELLER.to_lowercase()).unwrap();
        assert_eq!(gstin.as_str(), SELLER);
    }

    #[test]
    fn test_gstin_rejects_bad_checksum() {
        assert!(Gstin::parse("27AAPFU0939F1ZA").is_err());
    }

    #[test]
    fn test_gstin_rejects_malformed() {
        let cases = [
            "",
            "27AAPFU0939F1Z",                       // too short
            "27AAPFU0939F1ZVX",                     // too long
            &gstin_with_checksum("00AAPFU0939F1Z"), // state code 00
            &gstin_with_checksum("40AAPFU0939F1Z"), // unassigned state code
            &gstin_with_checksum("27AAPF10939F1Z"), // digit in PAN letters
            &gstin_with_checksum("27AAPFU09X9F1Z"), // letter in PAN digits
            &gstin_with_checksum("27AAPFU093991Z"), // PAN ends in a digit
            &gstin_with_checksum("27AAPFU0939F0Z"), // entity number 0
            &gstin_with_checksum("27AAPFU0939F1Y"), // 14th character not Z
            "27AAPFU0939F1Z-",                      // non-alphanumeric
        ];

        for case in cases {
            assert!(Gstin::parse(case).is_err(), "{case} should be rejected");
        }
    }

    #[test]
    fn test_gstin_accepts_special_state_codes() {
        for body in [
            "01AAPFU0939F1Z",
            "38AAPFU0939F1Z",
            "97AAPFU0939F1Z",
            "99AAPFU0939F1Z",
        ] {
            assert!(Gstin::parse(&gstin_with_checksum(body)).is_ok(), "{body}");
        }
    }

    #[test]
    fn test_gstin_serde_validates() {
        let json = serde_json::to_string(&Gstin::parse(SELLER).unwrap()).unwrap();
        assert_eq!(json, format!("\"{SELLER}\""));
        assert!(serde_json::from_str::<Gstin>(&json).is_ok());
        assert!(serde_json::from_str::<Gstin>("\"27AAPFU0939F1ZA\"").is_err());
    }

    #[test]
    fn test_line_item_validation() {
        let valid = |hsn: &str, qty: i32, price: i32, rate: i32| {
            GstLineItem::new(
                hsn.to_string(),
                "Item".to_string(),
                BigDecimal::from(qty),
                BigDecimal::from(price),
                BigDecimal::from(rate),
            )
            .is_ok()
        };

        assert!(valid("8471", 1, 100, 18));
        assert!(valid("847130", 1, 100, 18));
        assert!(valid("84713010", 1, 0, 0));
        assert!(!valid("847", 1, 100, 18));
        assert!(!valid("84713", 1, 100, 18));
        assert!(!valid("84A1", 1, 100, 18));
        assert!(!valid("8471", 0, 100, 18));
        assert!(!valid("8471", 1, -1, 18));
        assert!(!valid("8471", 1, 100, -5));
        assert!(!valid("8471", 1, 100, 101));
    }

    #[test]
    fn test_invoice_number_validation() {
        assert!(validate_invoice_number("INV/2024-25/001").is_ok());
        assert!(validate_invoice_number("").is_err());
        assert!(validate_invoice_number("INV/2024-25/00001").is_err()); // 17 chars
        assert!(validate_invoice_number("INV 001").is_err());
        assert!(validate_invoice_number("INV#001").is_err());
    }

    #[test]
    fn test_invoice_requires_line_items() {
        let result = GstInvoice::new(
            "INV-1".to_string(),
            NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
            Gstin::parse(SELLER).unwrap(),
            Gstin::parse(SELLER).unwrap(),
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_intra_state_breakdown() {
        let buyer = gstin_with_checksum("27AABCT1332L1Z");
        let invoice = invoice(&buyer, vec![item(18), item(5)]);
        assert!(!invoice.is_inter_state());

        let breakdown = invoice.breakdown().unwrap();
        assert_eq!(breakdown.taxable_value, BigDecimal::from(2000));
        assert_eq!(breakdown.cgst, BigDecimal::from(115)); // 90 + 25
        assert_eq!(breakdown.sgst, BigDecimal::from(115));
        assert_eq!(breakdown.igst, BigDecimal::from(0));
        assert_eq!(breakdown.total_tax, BigDecimal::from(230));
        assert_eq!(breakdown.total, BigDecimal::from(2230));
    }

    #[test]
    fn test_inter_state_breakdown() {
        let buyer = gstin_with_checksum("29AABCT1332L1Z");
        let invoice = invoice(&buyer, vec![item(18)]);
        assert!(invoice.is_inter_state());

        let lines = invoice.line_breakdowns().unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].igst, BigDecimal::from(180));

        let breakdown = invoice.breakdown().unwrap();
        assert_eq!(breakdown.cgst, BigDecimal::from(0));
        assert_eq!(breakdown.sgst, BigDecimal::from(0));
        assert_eq!(breakdown.igst, BigDecimal::from(180));
        assert_eq!(breakdown.total, BigDecimal::from(1180));
    }
}
