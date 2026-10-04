//! Printable view of a GST invoice
//!
//! [`InvoicePrint`] is everything a printed tax invoice shows, with every amount already formatted
//! in Indian digit grouping. It is built by the pure [`InvoicePrint::from_invoice`] from a
//! [`GstInvoice`] and the [`InvoiceParties`] that issue and receive it, and serialises to JSON as
//! is. The PDF renderer (behind the `pdf` feature) only lays this model out on the page.

use super::types::{GstBreakdown, GstInvoice, GstLineItem, Gstin, InvoiceError};
use crate::utils::formatting::{amount_in_words, format_inr};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Title printed at the top of a B2B GST invoice
pub const TAX_INVOICE_TITLE: &str = "Tax Invoice";
/// Date format printed on the invoice: day-month-year, as usual in India
pub const PRINT_DATE_FORMAT: &str = "%d-%m-%Y";
/// Taxable value, in rupees, from which an unregistered buyer's name and address must be on the
/// invoice (Rule 46 of the CGST Rules); below it they are printed only if the buyer asks
pub const UNREGISTERED_DETAILS_THRESHOLD_RUPEES: u32 = 50_000;

/// Name, address and GSTIN of a party to the invoice
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceParty {
    /// Legal or trade name
    pub name: String,
    /// Address lines, top to bottom
    pub address: Vec<String>,
    /// GSTIN, which must match the one on the invoice; `None` for an unregistered buyer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gstin: Option<Gstin>,
}

impl InvoiceParty {
    /// A registered party with its name, address lines and GSTIN
    pub fn new(name: impl Into<String>, address: Vec<String>, gstin: Gstin) -> Self {
        Self {
            name: name.into(),
            address,
            gstin: Some(gstin),
        }
    }

    /// An unregistered buyer with its name and address lines
    pub fn unregistered(name: impl Into<String>, address: Vec<String>) -> Self {
        Self {
            name: name.into(),
            address,
            gstin: None,
        }
    }

    /// An unregistered buyer who gave no name or address, allowed on an invoice whose taxable
    /// value is below [`UNREGISTERED_DETAILS_THRESHOLD_RUPEES`]
    #[must_use]
    pub fn walk_in() -> Self {
        Self::unregistered(String::new(), Vec::new())
    }

    /// Whether the party has a name that is not blank; a walk-in buyer has none
    #[must_use]
    pub fn has_name(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// Whether the party has an address line that is not blank
    fn has_address(&self) -> bool {
        self.address.iter().any(|line| !line.trim().is_empty())
    }
}

/// The supplier and the recipient of an invoice
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceParties {
    /// Supplier
    pub seller: InvoiceParty,
    /// Recipient
    pub buyer: InvoiceParty,
}

impl InvoiceParties {
    /// Pair a seller with a buyer
    #[must_use]
    pub fn new(seller: InvoiceParty, buyer: InvoiceParty) -> Self {
        Self { seller, buyer }
    }

    /// Check each party's details and that its GSTIN is the one on `invoice`, or that it has
    /// none when the invoice's buyer is unregistered
    ///
    /// The seller and a registered buyer need a name. An unregistered buyer needs a name and an
    /// address when the invoice's taxable value is [`UNREGISTERED_DETAILS_THRESHOLD_RUPEES`] or
    /// more, and neither below it (Rule 46), so [`InvoiceParty::walk_in`] passes there.
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidParty`] for the first party that fails, seller first, or the first
    /// error from [`GstInvoice::breakdown`].
    pub fn check_against(&self, invoice: &GstInvoice) -> Result<(), InvoiceError> {
        self.check_with_value(invoice, &invoice.breakdown()?.taxable_value)
    }

    /// [`InvoiceParties::check_against`] for an invoice whose taxable value is already known
    fn check_with_value(
        &self,
        invoice: &GstInvoice,
        taxable_value: &BigDecimal,
    ) -> Result<(), InvoiceError> {
        let seller = Some(&invoice.seller_gstin);
        check_party(PartyRole::Seller, &self.seller, seller, Details::Name)?;
        let threshold = BigDecimal::from(UNREGISTERED_DETAILS_THRESHOLD_RUPEES);
        let buyer_details = if invoice.buyer.is_registered() {
            Details::Name
        } else if taxable_value >= &threshold {
            Details::NameAndAddress
        } else {
            Details::Optional
        };
        check_party(
            PartyRole::Buyer,
            &self.buyer,
            invoice.buyer.gstin(),
            buyer_details,
        )
    }
}

/// Which of a party's details the invoice must show
#[derive(Clone, Copy)]
enum Details {
    /// None: an unregistered buyer below the Rule 46 threshold
    Optional,
    /// The name
    Name,
    /// The name and an address: an unregistered buyer at or above the Rule 46 threshold
    NameAndAddress,
}

/// Which side of the invoice a party is on
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartyRole {
    /// The supplier
    Seller,
    /// The recipient
    Buyer,
}

impl fmt::Display for PartyRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Seller => "seller",
            Self::Buyer => "buyer",
        })
    }
}

/// Why a party's details were rejected
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PartyError {
    /// The name is empty or only whitespace
    #[error("name cannot be empty")]
    EmptyName,
    /// An unregistered buyer has no address on an invoice at or above the Rule 46 threshold
    #[error("an unregistered buyer's address is required from a taxable value of {UNREGISTERED_DETAILS_THRESHOLD_RUPEES}")]
    MissingAddress,
    /// The party's GSTIN is not the one on the invoice
    #[error("GSTIN {found} does not match the invoice's {expected}")]
    GstinMismatch {
        /// GSTIN on the invoice
        expected: Gstin,
        /// GSTIN given with the party
        found: Gstin,
    },
    /// The invoice names a GSTIN for the party, but the party has none
    #[error("the invoice names GSTIN {expected} but the party has none")]
    MissingGstin {
        /// GSTIN on the invoice
        expected: Gstin,
    },
    /// The party has a GSTIN, but the invoice is to an unregistered buyer
    #[error("the party has GSTIN {found} but the invoice's buyer is unregistered")]
    UnexpectedGstin {
        /// GSTIN given with the party
        found: Gstin,
    },
}

/// Check one party's details, and its GSTIN against the one the invoice names for its role
fn check_party(
    role: PartyRole,
    party: &InvoiceParty,
    expected: Option<&Gstin>,
    details: Details,
) -> Result<(), InvoiceError> {
    let reason =
        details_error(party, details).or_else(|| gstin_error(expected, party.gstin.as_ref()));
    reason.map_or(Ok(()), |reason| {
        Err(InvoiceError::InvalidParty { role, reason })
    })
}

/// The first required detail the party lacks, if any
fn details_error(party: &InvoiceParty, details: Details) -> Option<PartyError> {
    let needs_name = matches!(details, Details::Name | Details::NameAndAddress);
    if needs_name && !party.has_name() {
        Some(PartyError::EmptyName)
    } else if matches!(details, Details::NameAndAddress) && !party.has_address() {
        Some(PartyError::MissingAddress)
    } else {
        None
    }
}

/// How a party's GSTIN differs from the one the invoice names, if it does
fn gstin_error(expected: Option<&Gstin>, found: Option<&Gstin>) -> Option<PartyError> {
    match (expected, found) {
        (Some(expected), Some(found)) if expected != found => Some(PartyError::GstinMismatch {
            expected: expected.clone(),
            found: found.clone(),
        }),
        (Some(expected), None) => Some(PartyError::MissingGstin {
            expected: expected.clone(),
        }),
        (None, Some(found)) => Some(PartyError::UnexpectedGstin {
            found: found.clone(),
        }),
        _ => None,
    }
}

/// One line of the item table, formatted for print
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintRow {
    /// Serial number, from 1
    pub serial: usize,
    /// HSN or SAC code
    pub hsn_sac: String,
    /// What was supplied
    pub description: String,
    /// Quantity
    pub quantity: String,
    /// Price per unit, before tax
    pub rate: String,
    /// Quantity times rate
    pub taxable_value: String,
    /// GST rate, e.g. `18%`
    pub gst_rate: String,
    /// GST charged on the line
    pub tax: String,
    /// Taxable value plus tax
    pub amount: String,
}

/// One tax line of the summary, e.g. CGST or IGST
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxLine {
    /// `CGST`, `SGST` or `IGST`
    pub label: String,
    /// Tax charged
    pub amount: String,
}

/// Everything a printed tax invoice shows, formatted
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoicePrint {
    /// Document title
    pub title: String,
    /// Invoice number
    pub invoice_number: String,
    /// Date of issue, `dd-mm-yyyy`
    pub invoice_date: String,
    /// State code of the place of supply: the buyer's GSTIN state for a B2B supply, or the
    /// state named on the invoice for an unregistered buyer
    pub place_of_supply: String,
    /// Whether IGST applies rather than CGST + SGST
    pub is_inter_state: bool,
    /// Supplier
    pub seller: InvoiceParty,
    /// Recipient
    pub buyer: InvoiceParty,
    /// Item table
    pub rows: Vec<PrintRow>,
    /// Total taxable value
    pub taxable_value: String,
    /// Tax lines with a non-zero amount
    pub tax_lines: Vec<TaxLine>,
    /// Total tax
    pub total_tax: String,
    /// Invoice total
    pub total: String,
    /// Invoice total in words
    pub total_in_words: String,
}

impl InvoicePrint {
    /// Build the printable view of `invoice`, issued by and to `parties`
    ///
    /// # Errors
    ///
    /// [`InvoiceError::InvalidParty`] if a party lacks a detail the invoice must show (see
    /// [`InvoiceParties::check_against`]) or its GSTIN is not the one on the invoice, or the
    /// first error from [`GstInvoice::line_breakdowns`].
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::invoice::{
    ///     GstInvoice, GstLineItem, Gstin, InvoiceParties, InvoiceParty, InvoicePrint,
    /// };
    /// use bigdecimal::BigDecimal;
    /// use chrono::NaiveDate;
    ///
    /// # fn main() -> Result<(), accounting_core::invoice::InvoiceError> {
    /// let seller = Gstin::parse("27AAPFU0939F1ZV")?;
    /// let buyer = Gstin::parse("29AAPFU0939F1ZR")?;
    /// let item = GstLineItem::new(
    ///     "998314",
    ///     "IT consulting",
    ///     BigDecimal::from(10),
    ///     BigDecimal::from(1500),
    ///     BigDecimal::from(18),
    /// )?;
    /// let invoice = GstInvoice::new(
    ///     "INV/2024-25/001",
    ///     NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
    ///     seller.clone(),
    ///     buyer.clone(),
    ///     vec![item],
    /// )?;
    /// let parties = InvoiceParties::new(
    ///     InvoiceParty::new("Acme Services", vec!["Mumbai".into()], seller),
    ///     InvoiceParty::new("Globex Ltd", vec!["Bengaluru".into()], buyer),
    /// );
    ///
    /// let print = InvoicePrint::from_invoice(&invoice, &parties)?;
    /// assert_eq!(print.invoice_date, "15-11-2024");
    /// assert_eq!(print.tax_lines[0].label, "IGST");
    /// assert_eq!(print.total, "17,700.00");
    /// assert_eq!(print.total_in_words, "Rupees Seventeen Thousand Seven Hundred Only");
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_invoice(
        invoice: &GstInvoice,
        parties: &InvoiceParties,
    ) -> Result<Self, InvoiceError> {
        let breakdowns = invoice.line_breakdowns()?;
        let totals: GstBreakdown = breakdowns.iter().sum();
        parties.check_with_value(invoice, &totals.taxable_value)?;
        let rows = invoice
            .line_items
            .iter()
            .zip(&breakdowns)
            .enumerate()
            .map(|(index, (item, breakdown))| print_row(index + 1, item, breakdown))
            .collect();

        Ok(Self {
            title: TAX_INVOICE_TITLE.to_string(),
            invoice_number: invoice.invoice_number.clone(),
            invoice_date: invoice.invoice_date.format(PRINT_DATE_FORMAT).to_string(),
            place_of_supply: invoice.buyer.place_of_supply().to_string(),
            is_inter_state: invoice.is_inter_state(),
            seller: parties.seller.clone(),
            buyer: parties.buyer.clone(),
            rows,
            taxable_value: format_inr(&totals.taxable_value),
            tax_lines: tax_lines(&totals),
            total_tax: format_inr(&totals.total_tax),
            total: format_inr(&totals.total),
            total_in_words: amount_in_words(&totals.total),
        })
    }
}

/// Format one line item with its tax breakdown
fn print_row(serial: usize, item: &GstLineItem, breakdown: &GstBreakdown) -> PrintRow {
    PrintRow {
        serial,
        hsn_sac: item.hsn_sac.clone(),
        description: item.description.clone(),
        quantity: plain_number(&item.quantity),
        rate: format_inr(&item.unit_price),
        taxable_value: format_inr(&breakdown.taxable_value),
        gst_rate: format!("{}%", plain_number(&item.gst_rate)),
        tax: format_inr(&breakdown.total_tax),
        amount: format_inr(&breakdown.total),
    }
}

/// The non-zero tax components of `totals`, in the order they are printed
fn tax_lines(totals: &GstBreakdown) -> Vec<TaxLine> {
    [
        ("CGST", &totals.cgst),
        ("SGST", &totals.sgst),
        ("IGST", &totals.igst),
    ]
    .into_iter()
    .filter(|(_, amount)| !amount.is_zero())
    .map(|(label, amount)| TaxLine {
        label: label.to_string(),
        amount: format_inr(amount),
    })
    .collect()
}

/// A number without trailing zeros or exponent, e.g. `10` or `2.5`
fn plain_number(value: &BigDecimal) -> String {
    value.normalized().to_plain_string()
}

/// How many item rows fit on each page, and how many row slots the totals need on the last one
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowCapacity {
    /// Rows on the first page, below the header and parties
    pub first_page: usize,
    /// Rows on every later page
    pub other_pages: usize,
    /// Row slots the totals block takes on the last page
    pub totals: usize,
}

/// Split `rows` into pages, adding an empty last page when the totals would not fit
///
/// Always returns at least one page. A capacity of zero is treated as one.
#[must_use]
pub fn paginate_rows(rows: &[PrintRow], capacity: RowCapacity) -> Vec<&[PrintRow]> {
    let (first, rest) = rows.split_at(capacity.first_page.max(1).min(rows.len()));
    let pages: Vec<&[PrintRow]> = std::iter::once(first)
        .chain(rest.chunks(capacity.other_pages.max(1)))
        .collect();
    let last_capacity = if pages.len() == 1 {
        capacity.first_page
    } else {
        capacity.other_pages
    };
    let last_len = pages.last().map_or(0, |page| page.len());
    if last_len + capacity.totals > last_capacity {
        pages.into_iter().chain(std::iter::once(&[][..])).collect()
    } else {
        pages
    }
}
