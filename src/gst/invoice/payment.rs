//! How the buyer can pay an invoice: bank details and a UPI payment link
//!
//! [`PaymentDetails`] is applied to a printed invoice with
//! [`InvoicePrint::with_payment`](super::print::InvoicePrint::with_payment). When it names a UPI
//! ID, the print model also carries a [`upi_uri`] that the PDF renderer draws as a QR code any UPI
//! app can scan. Everything here is pure.
//!
//! # Example
//!
//! ```
//! use accounting_core::gst::invoice::upi_uri;
//! use bigdecimal::BigDecimal;
//!
//! let uri = upi_uri("acme@okhdfc", "Acme Traders", &BigDecimal::from(1180), "INV0002");
//! assert_eq!(uri, "upi://pay?pa=acme%40okhdfc&pn=Acme%20Traders&am=1180.00&cu=INR&tn=INV0002");
//! ```

use crate::gst::tax::{round_to_paise, PAISE_SCALE};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

/// Currency code a UPI payment link asks for
pub const UPI_CURRENCY: &str = "INR";

/// Where the buyer can pay: bank account and UPI ID, each optional
///
/// A blank field counts as unset. With no field set, nothing is printed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentDetails {
    /// Account holder the payment goes to; the seller's name when unset
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    /// Name of the bank
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_name: Option<String>,
    /// Bank account number
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_number: Option<String>,
    /// IFSC of the bank branch
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ifsc: Option<String>,
    /// UPI ID (virtual payment address), such as `acme@okhdfc`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upi_id: Option<String>,
    /// Amount the UPI QR code asks for, such as the balance still owed on a part-paid invoice;
    /// the invoice total when unset. At zero or below no QR code is printed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_due: Option<BigDecimal>,
}

impl PaymentDetails {
    /// The details with every field trimmed and blank fields unset
    #[must_use]
    pub fn trimmed(self) -> Self {
        Self {
            payee_name: trim(self.payee_name),
            bank_name: trim(self.bank_name),
            account_number: trim(self.account_number),
            ifsc: trim(self.ifsc),
            upi_id: trim(self.upi_id),
            amount_due: self.amount_due,
        }
    }

    /// Whether no bank or UPI detail is set, ignoring blank fields, the payee name and the
    /// amount due
    #[must_use]
    pub fn is_empty(&self) -> bool {
        [
            &self.bank_name,
            &self.account_number,
            &self.ifsc,
            &self.upi_id,
        ]
        .into_iter()
        .all(|field| field.as_deref().is_none_or(|text| text.trim().is_empty()))
    }
}

/// `field` trimmed, or `None` when it is blank
fn trim(field: Option<String>) -> Option<String> {
    field.as_deref().and_then(non_blank)
}

/// `text` trimmed, or `None` when it is blank
pub(super) fn non_blank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A UPI payment link asking `upi_id` for `amount` rupees, with `note` as the transaction note
///
/// The amount is rounded to the paisa and written with two decimals. Every value is
/// percent-encoded, so the link is safe to put in a QR code as is.
#[must_use]
pub fn upi_uri(
    upi_id: impl Into<String>,
    payee_name: impl Into<String>,
    amount: &BigDecimal,
    note: impl Into<String>,
) -> String {
    let amount = round_to_paise(amount).with_scale(PAISE_SCALE);
    format!(
        "upi://pay?pa={}&pn={}&am={}&cu={UPI_CURRENCY}&tn={}",
        percent_encode(&upi_id.into()),
        percent_encode(&payee_name.into()),
        amount.to_plain_string(),
        percent_encode(&note.into()),
    )
}

/// `text` with every byte other than an RFC 3986 unreserved character written as `%XX`
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
