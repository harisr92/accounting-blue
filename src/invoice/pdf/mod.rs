//! PDF rendering of GST invoices (needs the `pdf` feature)
//!
//! [`render_pdf`] lays an [`InvoicePrint`] out on A4 pages and returns the PDF bytes;
//! [`GstInvoice::to_pdf`] builds the print model and renders it in one call. The page shows a
//! logo placeholder, the seller and invoice details, the buyer, the item table, the tax summary
//! with the total in words, any terms, a signature line and a footer note with page numbers. Item
//! tables longer than a page continue on the next one, and descriptions too long for their
//! column are cut short with an ellipsis.
//!
//! By default the invoice is set in the standard Helvetica font and amounts are labelled `Rs.`.
//! Helvetica only covers Western European characters, so an invoice with the `₹` sign or with
//! names in an Indian script needs a [`PdfFont::Custom`] TrueType font that has those glyphs.
//! Rendering refuses any character its font can't draw with [`PdfError::MissingGlyph`], whether it
//! comes from the invoice or is one of the renderer's own labels. The terms print on the same page
//! as the totals, so terms longer than that page holds are refused with
//! [`PdfError::TermsTooLong`].
//!
//! # Example
//!
//! ```
//! use accounting_core::invoice::{
//!     GstInvoice, GstLineItem, Gstin, InvoiceParties, InvoiceParty, PdfOptions,
//! };
//! use bigdecimal::BigDecimal;
//! use chrono::NaiveDate;
//!
//! # fn main() -> Result<(), accounting_core::Error> {
//! let seller = Gstin::parse("27AAPFU0939F1ZV")?;
//! let buyer = Gstin::parse("27AAPFU0939F2ZU")?;
//! let item = GstLineItem::new(
//!     "998314",
//!     "IT consulting",
//!     BigDecimal::from(10),
//!     BigDecimal::from(1500),
//!     BigDecimal::from(18),
//! )?;
//! let invoice = GstInvoice::new(
//!     "INV/2024-25/001",
//!     NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
//!     seller.clone(),
//!     buyer.clone(),
//!     vec![item],
//! )?;
//! let parties = InvoiceParties::new(
//!     InvoiceParty::new("Acme Services", vec!["Mumbai".into()], seller),
//!     InvoiceParty::new("Globex Ltd", vec!["Pune".into()], buyer),
//! );
//!
//! let pdf = invoice.to_pdf(&parties, &PdfOptions::default())?;
//! assert!(pdf.starts_with(b"%PDF-"));
//! # Ok(())
//! # }
//! ```

mod draw;
mod fonts;
mod layout;
mod text;

use super::print::{paginate_rows, InvoiceParties, InvoicePrint};
use super::types::{GstInvoice, InvoiceError};
use draw::{PAGE_HEIGHT, PAGE_WIDTH};
use fonts::{Face, Fonts};
use layout::Layout;
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, TextItem};
use std::fmt;

/// Currency label printed before amounts when the built-in font is used
pub const DEFAULT_CURRENCY_LABEL: &str = "Rs.";
/// Note printed at the foot of every page by default; left out when the buyer is unregistered,
/// since e-invoicing applies only to B2B supplies
pub const DEFAULT_FOOTER_NOTE: &str = "E-Invoice Ready";

/// The font an invoice is set in
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PdfFont {
    /// Standard Helvetica and Helvetica Bold, not embedded; Western European characters only
    #[default]
    Builtin,
    /// TrueType or OpenType font files, embedded in the PDF
    Custom {
        /// Regular face
        regular: Vec<u8>,
        /// Bold face; the regular face is used when absent
        bold: Option<Vec<u8>>,
    },
}

/// How an invoice is rendered
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfOptions {
    /// Font to set the invoice in
    pub font: PdfFont,
    /// Printed before each total, e.g. `Rs.` or `₹`; empty for none
    pub currency_label: String,
    /// Terms and conditions, one paragraph each, printed above the page foot on the last page
    pub terms: Vec<String>,
    /// Note printed at the foot of every page. The default, [`DEFAULT_FOOTER_NOTE`], is left out
    /// for an unregistered buyer; any other note is always printed.
    pub footer_note: String,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            font: PdfFont::Builtin,
            currency_label: DEFAULT_CURRENCY_LABEL.to_string(),
            terms: Vec::new(),
            footer_note: DEFAULT_FOOTER_NOTE.to_string(),
        }
    }
}

/// Which face of a [`PdfFont::Custom`] font
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFace {
    /// The regular face
    Regular,
    /// The bold face
    Bold,
}

impl fmt::Display for FontFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Regular => "regular",
            Self::Bold => "bold",
        })
    }
}

/// Why an invoice could not be rendered to PDF
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The invoice or its parties are invalid
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
    /// The bytes given for a custom font are not a TrueType or OpenType font
    #[error("the {face} font is not a valid TrueType or OpenType font")]
    InvalidFont {
        /// Which face was rejected
        face: FontFace,
    },
    /// The metrics printpdf ships for the built-in fonts could not be loaded
    #[error("could not load the metrics of the built-in font")]
    BuiltinFontMetrics,
    /// The text contains a character the font has no glyph for
    #[error("the font has no glyph for {ch:?}; use a font that covers it")]
    MissingGlyph {
        /// The first character that can't be drawn
        ch: char,
    },
    /// The terms, once wrapped, don't fit below the totals on one page
    #[error("the terms take {lines} lines but at most {max_lines} fit below the totals")]
    TermsTooLong {
        /// Lines the terms wrap to
        lines: usize,
        /// Lines that fit
        max_lines: usize,
    },
}

/// Render the printable view of an invoice as A4 PDF bytes
///
/// # Errors
///
/// [`PdfError::InvalidFont`] if custom font bytes don't parse, [`PdfError::MissingGlyph`] if the
/// font can't draw a character on the page, [`PdfError::TermsTooLong`] if the terms don't fit on
/// the last page, and
/// [`PdfError::BuiltinFontMetrics`] if the built-in font metrics are unavailable.
pub fn render_pdf(print: &InvoicePrint, options: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    let mut document = PdfDocument::new(&format!("{} {}", print.title, print.invoice_number));
    let fonts = Fonts::load(&mut document, &options.font)?;
    let layout = Layout::new(&fonts, print, options);
    layout.check_terms_fit()?;

    let pages = paginate_rows(&print.rows, layout.capacity());
    let count = pages.len();
    let page_ops: Vec<Vec<Op>> = pages
        .iter()
        .enumerate()
        .map(|(index, rows)| layout.page(index + 1, count, rows))
        .collect();
    check_glyphs(&fonts, &page_ops)?;
    let pages = page_ops
        .into_iter()
        .map(|ops| PdfPage::new(Mm(PAGE_WIDTH), Mm(PAGE_HEIGHT), ops))
        .collect();

    Ok(document
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new()))
}

impl GstInvoice {
    /// Render this invoice, issued by and to `parties`, as A4 PDF bytes
    ///
    /// # Errors
    ///
    /// [`PdfError::Invoice`] from [`InvoicePrint::from_invoice`], or any error from
    /// [`render_pdf`].
    pub fn to_pdf(
        &self,
        parties: &InvoiceParties,
        options: &PdfOptions,
    ) -> Result<Vec<u8>, PdfError> {
        render_pdf(&InvoicePrint::from_invoice(self, parties)?, options)
    }
}

/// Refuse text the fonts can't draw, rather than print blank boxes
///
/// Walks the page ops as drawn, so labels, numbers and caller text are all checked against the
/// face they are set in.
fn check_glyphs(fonts: &Fonts, pages: &[Vec<Op>]) -> Result<(), PdfError> {
    let missing = pages
        .iter()
        .flatten()
        .scan(None::<&Face>, |face, op| {
            match op {
                Op::SetFont { font, .. } => *face = fonts.face(font),
                Op::ShowText { items } => {
                    let unknown = face.and_then(|face| first_missing(face, items));
                    return Some(unknown);
                }
                _ => {}
            }
            Some(None)
        })
        .flatten()
        .next();
    missing.map_or(Ok(()), |ch| Err(PdfError::MissingGlyph { ch }))
}

/// The first non-whitespace character in `items` that `face` has no glyph for
fn first_missing(face: &Face, items: &[TextItem]) -> Option<char> {
    items
        .iter()
        .filter_map(|item| match item {
            TextItem::Text(text) => Some(text),
            _ => None,
        })
        .flat_map(|text| text.chars())
        .find(|c| !c.is_whitespace() && !face.has_glyph(*c))
}

#[cfg(test)]
mod tests;
