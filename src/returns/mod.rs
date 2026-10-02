//! GST returns built from invoices
//!
//! [`Gstr1Return::build`] aggregates a filer's [`GstInvoice`](crate::invoice::GstInvoice)s for one
//! [`ReturnPeriod`] into GSTR-1, the return of outward supplies: Table 4A (B2B supplies by buyer
//! and rate), Table 5 (B2CL: large inter-state supplies to unregistered buyers, by place of
//! supply), Table 12 (the HSN/SAC summary) and Table 13 (documents issued).
//! [`Gstr1Return::to_json`] writes it in the GST portal's offline-tool schema for upload.
//!
//! Building is pure: the caller passes the invoices and the HSN/SAC master, and gets a value
//! back. B2CS supplies (any other supply to an unregistered buyer) are refused, and exports,
//! credit and debit notes and amendments are not covered yet. Failures are typed as [`Gstr1Error`].
//!
//! # Example
//!
//! ```
//! use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin, HsnMaster};
//! use accounting_core::returns::{Gstr1Return, ReturnPeriod};
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # fn main() -> Result<(), accounting_core::Error> {
//! let seller = Gstin::parse("27AAPFU0939F1ZV")?;
//! let buyer = Gstin::parse("27AAPFU0939F2ZU")?;
//! let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
//! let item = |rate| GstLineItem::new("998314", "IT consulting", BigDecimal::from(1),
//!     BigDecimal::from(1000), BigDecimal::from(rate));
//! let invoices = [
//!     GstInvoice::new("INV-001", date, seller.clone(), buyer.clone(), vec![item(18)?])?,
//!     GstInvoice::new("INV-002", date, seller.clone(), buyer, vec![item(18)?, item(5)?])?,
//! ];
//!
//! let gstr1 = Gstr1Return::build(&seller, ReturnPeriod::new(2024, 11)?, &invoices,
//!     HsnMaster::global())?;
//!
//! // One buyer with two invoices; the second has one item per rate
//! assert_eq!(gstr1.b2b.len(), 1);
//! assert_eq!(gstr1.b2b[0].invoices[1].items.len(), 2);
//! // The HSN summary has one row per code and rate
//! assert_eq!(gstr1.hsn.b2b.len(), 2);
//! assert_eq!(gstr1.doc_issue.documents[0].series[0].total, 2);
//! # Ok(())
//! # }
//! ```

pub mod gstr1;
pub mod period;

pub use gstr1::{
    B2bInvoice, B2bInvoiceType, B2bItem, B2bParty, B2clInvoice, B2clItem, B2clItemDetail,
    B2clPlace, DocIssue, DocSeries, DocSummary, Gstr1Error, Gstr1Return, HsnRow, HsnSummary,
    ItemDetail, GOODS_UQC, OUTWARD_INVOICES_DOC_TYPE, SERVICES_UQC,
};
pub use period::{ReturnPeriod, FIRST_GST_YEAR};

#[cfg(test)]
mod tests;
