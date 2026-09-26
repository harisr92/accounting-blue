//! GST invoices under Indian rules
//!
//! [`GstInvoice`] models a B2B tax invoice: invoice number, date, seller and buyer [`Gstin`], and
//! [`GstLineItem`]s carrying an HSN/SAC code. Every identifier is validated on construction, so an
//! invoice that exists is well-formed. Whether tax is split as CGST + SGST or charged as IGST
//! follows from the seller and buyer state codes; [`GstInvoice::breakdown`] returns the totals as
//! a [`GstBreakdown`].
//!
//! [`HsnMaster`] holds common HSN/SAC codes with their default GST rates;
//! [`GstLineItem::with_default_rate`] builds a line at the rate for its code.
//!
//! The tax arithmetic comes from [`crate::tax::gst`]. The invoice types are also re-exported at the
//! crate root.
//!
//! # Example
//!
//! ```
//! use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin};
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # fn main() -> Result<(), accounting_core::invoice::InvoiceError> {
//! let item = GstLineItem::new(
//!     "998314".to_string(),
//!     "IT consulting".to_string(),
//!     BigDecimal::from(10),
//!     BigDecimal::from(1500),
//!     BigDecimal::from(18),
//! )?;
//!
//! let invoice = GstInvoice::new(
//!     "INV/2024-25/001".to_string(),
//!     NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
//!     Gstin::parse("27AAPFU0939F1ZV")?,
//!     Gstin::parse("27AAPFU0939F2ZU")?,
//!     vec![item],
//! )?;
//!
//! let breakdown = invoice.breakdown()?;
//! assert_eq!(breakdown.cgst, BigDecimal::from(1350));
//! assert_eq!(breakdown.sgst, BigDecimal::from(1350));
//! assert_eq!(breakdown.total, BigDecimal::from(17700));
//! # Ok(())
//! # }
//! ```

pub mod hsn_lookup;
pub mod types;

pub use hsn_lookup::{HsnMaster, HsnSacEntry, HsnSacKind};
pub use types::*;
