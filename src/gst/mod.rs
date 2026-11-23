//! Indian GST, behind the `india` feature
//!
//! Everything specific to India's Goods and Services Tax, built on the country-neutral invoicing
//! core:
//!
//! - [`tax`]: GST rates, CGST/SGST/IGST calculations and [`tax::GstRegime`], the GST tax regime
//! - [`invoice`]: GSTIN, HSN/SAC master data, GST invoices and credit notes, compliance checks,
//!   the print model and, with the `pdf` feature, the PDF renderer
//! - [`returns`]: GSTR-1 built from invoices and credit notes, in the GST portal's JSON schema

pub mod invoice;
pub mod returns;
pub mod tax;
