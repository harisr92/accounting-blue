//! GSTR-1: the return of outward supplies
//!
//! [`Gstr1Return::build`] aggregates a filer's B2B invoices for one [`ReturnPeriod`] into the
//! sections of GSTR-1 this crate can fill: Table 4A (B2B supplies, grouped by buyer GSTIN with
//! one item per rate), Table 12 (the HSN/SAC summary, grouped by code and rate) and Table 13
//! (documents issued). The builder is pure; [`Gstr1Return::to_json`] writes the result in the
//! GST portal's offline-tool schema, with its short keys and amounts as JSON numbers rounded to
//! paise, ready to upload.
//!
//! Table 12 rows go under `hsn.hsn_b2b`, the B2B tab of the HSN summary the portal has used
//! since it split Table 12 into B2B and B2C tabs.

use super::period::ReturnPeriod;
use crate::invoice::print::PRINT_DATE_FORMAT;
use crate::invoice::validation::compliance_errors;
use crate::invoice::{
    ComplianceIssue, GstBreakdown, GstInvoice, Gstin, HsnMaster, HsnSacKind, InvoiceError,
};
use crate::tax::round_to_paise;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use chrono::NaiveDate;
use serde::ser::Error as _;
use serde::{Serialize, Serializer};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

/// Unit quantity code reported for goods in the HSN summary; line items carry no unit, so `OTH`
/// (others)
pub const GOODS_UQC: &str = "OTH";

/// Unit quantity code reported for services in the HSN summary: `NA`, with a quantity of 0
pub const SERVICES_UQC: &str = "NA";

/// Table 13 name for the tax invoices this return reports
pub const OUTWARD_INVOICES_DOC_TYPE: &str = "Invoices for outward supply";

/// Table 13 serial number for invoices for outward supply
const OUTWARD_INVOICES_DOC_NUM: usize = 1;

/// A GSTR-1 return for one filer and period
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gstr1Return {
    /// GSTIN of the filer, who issued every invoice
    #[serde(rename = "gstin")]
    pub filer_gstin: Gstin,
    /// Tax period filed for
    #[serde(rename = "fp")]
    pub period: ReturnPeriod,
    /// Table 4A: invoices to registered buyers, one entry per buyer GSTIN
    #[serde(rename = "b2b", skip_serializing_if = "Vec::is_empty")]
    pub b2b: Vec<B2bParty>,
    /// Table 12: HSN/SAC summary
    #[serde(rename = "hsn", skip_serializing_if = "HsnSummary::is_empty")]
    pub hsn: HsnSummary,
    /// Table 13: documents issued in the period
    #[serde(rename = "doc_issue", skip_serializing_if = "DocIssue::is_empty")]
    pub doc_issue: DocIssue,
}

/// Every invoice to one registered buyer
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2bParty {
    /// Buyer's GSTIN
    #[serde(rename = "ctin")]
    pub buyer_gstin: Gstin,
    /// The buyer's invoices, by date then number
    #[serde(rename = "inv")]
    pub invoices: Vec<B2bInvoice>,
}

/// One invoice in Table 4A
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2bInvoice {
    /// Invoice number
    #[serde(rename = "inum")]
    pub invoice_number: String,
    /// Date of issue, written `dd-mm-yyyy`
    #[serde(rename = "idt", serialize_with = "portal_date")]
    pub invoice_date: NaiveDate,
    /// Invoice value: taxable value plus tax
    #[serde(rename = "val", serialize_with = "portal_amount")]
    pub invoice_value: BigDecimal,
    /// Place of supply: the buyer's state code
    #[serde(rename = "pos")]
    pub place_of_supply: String,
    /// Whether tax is payable on reverse charge, written `Y` or `N`
    #[serde(rename = "rchrg", serialize_with = "yes_no")]
    pub reverse_charge: bool,
    /// Kind of invoice
    #[serde(rename = "inv_typ")]
    pub invoice_type: B2bInvoiceType,
    /// One item per GST rate on the invoice, by rate
    #[serde(rename = "itms")]
    pub items: Vec<B2bItem>,
}

/// Kind of a Table 4A invoice
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum B2bInvoiceType {
    /// A regular tax invoice
    #[serde(rename = "R")]
    Regular,
}

/// The lines of one invoice at one GST rate
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2bItem {
    /// Serial number within the invoice, from 1
    #[serde(rename = "num")]
    pub number: usize,
    /// Taxable value and tax at this rate
    #[serde(rename = "itm_det")]
    pub detail: ItemDetail,
}

/// Taxable value and tax of the lines at one rate
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemDetail {
    /// Total GST rate as a percentage
    #[serde(rename = "rt", serialize_with = "portal_number")]
    pub rate: BigDecimal,
    /// Taxable value
    #[serde(rename = "txval", serialize_with = "portal_amount")]
    pub taxable_value: BigDecimal,
    /// Integrated GST
    #[serde(rename = "iamt", serialize_with = "portal_amount")]
    pub igst: BigDecimal,
    /// Central GST
    #[serde(rename = "camt", serialize_with = "portal_amount")]
    pub cgst: BigDecimal,
    /// State GST
    #[serde(rename = "samt", serialize_with = "portal_amount")]
    pub sgst: BigDecimal,
    /// Compensation cess, which this crate does not charge
    #[serde(rename = "csamt", serialize_with = "portal_amount")]
    pub cess: BigDecimal,
}

impl ItemDetail {
    fn new(rate: BigDecimal, breakdown: &GstBreakdown) -> Self {
        Self {
            rate,
            taxable_value: breakdown.taxable_value.clone(),
            igst: breakdown.igst.clone(),
            cgst: breakdown.cgst.clone(),
            sgst: breakdown.sgst.clone(),
            cess: BigDecimal::zero(),
        }
    }
}

/// Table 12: supplies summarised by HSN/SAC code and rate
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HsnSummary {
    /// Rows for supplies to registered buyers, by code then rate
    #[serde(rename = "hsn_b2b")]
    pub b2b: Vec<HsnRow>,
}

impl HsnSummary {
    /// Whether the summary has no rows
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.b2b.is_empty()
    }
}

/// Every line with one HSN/SAC code and rate
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HsnRow {
    /// Serial number, from 1
    #[serde(rename = "num")]
    pub number: usize,
    /// HSN or SAC code
    #[serde(rename = "hsn_sc")]
    pub hsn_sac: String,
    /// Description from the HSN/SAC master, or of the first line if the master lacks the code
    #[serde(rename = "desc")]
    pub description: String,
    /// Unit quantity code: [`GOODS_UQC`] for goods, [`SERVICES_UQC`] for services
    #[serde(rename = "uqc")]
    pub uqc: String,
    /// Total quantity for goods; 0 for services, which the portal reports without a quantity
    #[serde(rename = "qty", serialize_with = "portal_amount")]
    pub quantity: BigDecimal,
    /// Total value: taxable value plus tax
    #[serde(rename = "val", serialize_with = "portal_amount")]
    pub total_value: BigDecimal,
    /// Taxable value
    #[serde(rename = "txval", serialize_with = "portal_amount")]
    pub taxable_value: BigDecimal,
    /// Integrated GST
    #[serde(rename = "iamt", serialize_with = "portal_amount")]
    pub igst: BigDecimal,
    /// Central GST
    #[serde(rename = "camt", serialize_with = "portal_amount")]
    pub cgst: BigDecimal,
    /// State GST
    #[serde(rename = "samt", serialize_with = "portal_amount")]
    pub sgst: BigDecimal,
    /// Compensation cess, which this crate does not charge
    #[serde(rename = "csamt", serialize_with = "portal_amount")]
    pub cess: BigDecimal,
    /// Total GST rate as a percentage
    #[serde(rename = "rt", serialize_with = "portal_number")]
    pub rate: BigDecimal,
}

/// Table 13: documents issued in the period
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DocIssue {
    /// One entry per kind of document
    #[serde(rename = "doc_det")]
    pub documents: Vec<DocSummary>,
}

impl DocIssue {
    /// Whether no documents were issued
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

/// The documents of one kind
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocSummary {
    /// Table 13 serial number of the kind
    #[serde(rename = "doc_num")]
    pub doc_num: usize,
    /// Name of the kind
    #[serde(rename = "doc_typ")]
    pub doc_type: String,
    /// Number series used
    #[serde(rename = "docs")]
    pub series: Vec<DocSeries>,
}

/// A run of document numbers
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocSeries {
    /// Serial number, from 1
    #[serde(rename = "num")]
    pub number: usize,
    /// First document number, by date then number
    #[serde(rename = "from")]
    pub from: String,
    /// Last document number, by date then number
    #[serde(rename = "to")]
    pub to: String,
    /// Documents issued
    #[serde(rename = "totnum")]
    pub total: usize,
    /// Documents cancelled, which invoices here never are
    #[serde(rename = "cancel")]
    pub cancelled: usize,
    /// Issued less cancelled
    #[serde(rename = "net_issue")]
    pub net_issued: usize,
}

/// Why a GSTR-1 return could not be built or written
#[derive(Debug, thiserror::Error)]
pub enum Gstr1Error {
    /// The month is not 1-12, or the year is outside the GST era
    #[error("invalid return period: month {month} of {year}")]
    InvalidPeriod {
        /// Year given
        year: i32,
        /// Month given
        month: u32,
    },
    /// The invoice was issued by someone other than the filer
    #[error("invoice {invoice_number} was issued by {seller}, not the filer")]
    SellerMismatch {
        /// The invoice number
        invoice_number: String,
        /// GSTIN of its seller
        seller: Gstin,
    },
    /// The invoice is dated outside the return period
    #[error("invoice {invoice_number} dated {date} is outside the period {period}")]
    OutsidePeriod {
        /// The invoice number
        invoice_number: String,
        /// Its date
        date: NaiveDate,
        /// The period being filed
        period: ReturnPeriod,
    },
    /// Two invoices share a number, compared case-insensitively
    #[error("invoice number {0} appears more than once")]
    DuplicateInvoiceNumber(String),
    /// The invoice breaks an error-severity compliance rule
    #[error("invoice {invoice_number} is not compliant: {} error(s)", issues.len())]
    NotCompliant {
        /// The invoice number
        invoice_number: String,
        /// The error-severity issues found
        issues: Vec<ComplianceIssue>,
    },
    /// An invoice's tax could not be computed
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
    /// The return could not be written as JSON
    #[error("could not write GSTR-1 JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// An invoice with the tax breakdown of each of its lines, computed once
struct PricedInvoice<'a> {
    invoice: &'a GstInvoice,
    lines: Vec<GstBreakdown>,
}

impl Gstr1Return {
    /// Aggregate a filer's invoices for a period into GSTR-1
    ///
    /// Every invoice must be issued by `filer`, dated in `period`, carry a number no other
    /// invoice has (ignoring case), and pass the error-severity rules of
    /// [`validate_invoice`](crate::invoice::validate_invoice) as of its own date. `master`
    /// supplies the HSN/SAC summary's descriptions, usually [`HsnMaster::global`]. Output is
    /// ordered by buyer GSTIN, invoice date and number, and rate, whatever the input order.
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::SellerMismatch`], [`Gstr1Error::OutsidePeriod`],
    /// [`Gstr1Error::DuplicateInvoiceNumber`] or [`Gstr1Error::NotCompliant`] for the first
    /// invoice that fails, or [`Gstr1Error::Invoice`] if a line's tax can't be computed.
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin, HsnMaster};
    /// use accounting_core::returns::{Gstr1Return, ReturnPeriod};
    /// use bigdecimal::BigDecimal;
    /// use chrono::NaiveDate;
    ///
    /// # fn main() -> Result<(), accounting_core::Error> {
    /// let seller = Gstin::parse("27AAPFU0939F1ZV")?;
    /// let item = GstLineItem::new("998314", "IT consulting", BigDecimal::from(1),
    ///     BigDecimal::from(1000), BigDecimal::from(18))?;
    /// let invoice = GstInvoice::new("INV-001", NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
    ///     seller.clone(), Gstin::parse("29AAPFU0939F1ZR")?, vec![item])?;
    ///
    /// let period = ReturnPeriod::new(2024, 11)?;
    /// let gstr1 = Gstr1Return::build(&seller, period, &[invoice], HsnMaster::global())?;
    /// assert_eq!(gstr1.b2b[0].invoices[0].invoice_value, BigDecimal::from(1180));
    ///
    /// let json = gstr1.to_json()?;
    /// assert!(json.contains(r#""fp": "112024""#));
    /// assert!(json.contains(r#""iamt": 180.0"#));
    /// # Ok(())
    /// # }
    /// ```
    pub fn build(
        filer: &Gstin,
        period: ReturnPeriod,
        invoices: &[GstInvoice],
        master: &HsnMaster,
    ) -> Result<Self, Gstr1Error> {
        check_invoices(filer, period, invoices)?;

        let mut priced = invoices
            .iter()
            .map(|invoice| {
                Ok(PricedInvoice {
                    invoice,
                    lines: invoice.line_breakdowns()?,
                })
            })
            .collect::<Result<Vec<_>, InvoiceError>>()?;
        priced.sort_by(|a, b| invoice_order(a.invoice, b.invoice));

        Ok(Self {
            filer_gstin: filer.clone(),
            period,
            b2b: b2b_section(&priced),
            hsn: hsn_section(&priced, master),
            doc_issue: doc_issue_section(&priced),
        })
    }

    /// The return as pretty-printed JSON in the portal's offline-tool schema
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::Json`] if an amount is too large to write as a JSON number.
    pub fn to_json(&self) -> Result<String, Gstr1Error> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Invoices by date, then number with its trailing digits compared as a number, so that
/// `INV-9` comes before `INV-10`
fn invoice_order(a: &GstInvoice, b: &GstInvoice) -> Ordering {
    (a.invoice_date, number_key(&a.invoice_number))
        .cmp(&(b.invoice_date, number_key(&b.invoice_number)))
        .then_with(|| a.invoice_number.cmp(&b.invoice_number))
}

/// An invoice number as its prefix and its trailing digits, the digits keyed by their
/// significant length then text so that they compare as a number of any size
fn number_key(number: &str) -> (&str, usize, &str) {
    let prefix = number.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = number
        .strip_prefix(prefix)
        .unwrap_or_default()
        .trim_start_matches('0');
    (prefix, digits.len(), digits)
}

/// Check every invoice, and that no two share a number
fn check_invoices(
    filer: &Gstin,
    period: ReturnPeriod,
    invoices: &[GstInvoice],
) -> Result<(), Gstr1Error> {
    invoices
        .iter()
        .try_fold(HashSet::new(), |mut seen, invoice| {
            check_invoice(filer, period, invoice)?;
            if seen.insert(invoice.invoice_number.to_ascii_uppercase()) {
                Ok(seen)
            } else {
                Err(Gstr1Error::DuplicateInvoiceNumber(
                    invoice.invoice_number.clone(),
                ))
            }
        })
        .map(drop)
}

/// The rules one invoice must meet on its own
fn check_invoice(
    filer: &Gstin,
    period: ReturnPeriod,
    invoice: &GstInvoice,
) -> Result<(), Gstr1Error> {
    let invoice_number = || invoice.invoice_number.clone();
    if invoice.seller_gstin != *filer {
        return Err(Gstr1Error::SellerMismatch {
            invoice_number: invoice_number(),
            seller: invoice.seller_gstin.clone(),
        });
    }
    if !period.contains(invoice.invoice_date) {
        return Err(Gstr1Error::OutsidePeriod {
            invoice_number: invoice_number(),
            date: invoice.invoice_date,
            period,
        });
    }
    let issues = compliance_errors(invoice, invoice.invoice_date);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(Gstr1Error::NotCompliant {
            invoice_number: invoice_number(),
            issues,
        })
    }
}

/// Table 4A: invoices grouped by buyer GSTIN, keeping their order within each buyer
fn b2b_section(priced: &[PricedInvoice]) -> Vec<B2bParty> {
    priced
        .iter()
        .fold(
            BTreeMap::<&Gstin, Vec<B2bInvoice>>::new(),
            |mut parties, p| {
                parties
                    .entry(&p.invoice.buyer_gstin)
                    .or_default()
                    .push(b2b_invoice(p));
                parties
            },
        )
        .into_iter()
        .map(|(buyer_gstin, invoices)| B2bParty {
            buyer_gstin: buyer_gstin.clone(),
            invoices,
        })
        .collect()
}

fn b2b_invoice(p: &PricedInvoice) -> B2bInvoice {
    let total: GstBreakdown = p.lines.iter().sum();
    B2bInvoice {
        invoice_number: p.invoice.invoice_number.clone(),
        invoice_date: p.invoice.invoice_date,
        invoice_value: total.total,
        place_of_supply: p.invoice.buyer_gstin.state_code().to_string(),
        reverse_charge: false,
        invoice_type: B2bInvoiceType::Regular,
        items: by_rate(p)
            .into_iter()
            .enumerate()
            .map(|(i, (rate, breakdown))| B2bItem {
                number: i + 1,
                detail: ItemDetail::new(rate, &breakdown),
            })
            .collect(),
    }
}

/// An invoice's line breakdowns summed per rate, so that 18 and 18.00 are one rate
fn by_rate(p: &PricedInvoice) -> BTreeMap<BigDecimal, GstBreakdown> {
    p.invoice.line_items.iter().zip(&p.lines).fold(
        BTreeMap::new(),
        |mut rates, (item, breakdown)| {
            *rates.entry(item.gst_rate.normalized()).or_default() += breakdown;
            rates
        },
    )
}

/// The lines of every invoice with one HSN/SAC code and rate, before numbering
#[derive(Default)]
struct HsnTotals {
    quantity: BigDecimal,
    breakdown: GstBreakdown,
    first_description: String,
}

/// Table 12: every line grouped by HSN/SAC code and rate
fn hsn_section(priced: &[PricedInvoice], master: &HsnMaster) -> HsnSummary {
    let totals = priced
        .iter()
        .flat_map(|p| p.invoice.line_items.iter().zip(&p.lines))
        .fold(BTreeMap::new(), |mut rows, (item, breakdown)| {
            let totals: &mut HsnTotals = rows
                .entry((item.hsn_sac.clone(), item.gst_rate.normalized()))
                .or_default();
            if totals.first_description.is_empty() {
                totals.first_description.clone_from(&item.description);
            }
            totals.quantity += &item.quantity;
            totals.breakdown += breakdown;
            rows
        });

    HsnSummary {
        b2b: totals
            .into_iter()
            .enumerate()
            .map(|(i, ((hsn_sac, rate), totals))| hsn_row(i + 1, hsn_sac, rate, totals, master))
            .collect(),
    }
}

fn hsn_row(
    number: usize,
    hsn_sac: String,
    rate: BigDecimal,
    totals: HsnTotals,
    master: &HsnMaster,
) -> HsnRow {
    let description = master
        .lookup(&hsn_sac)
        .map_or(totals.first_description, |entry| entry.description.clone());
    let (uqc, quantity) = match HsnSacKind::of_code(&hsn_sac) {
        HsnSacKind::Hsn => (GOODS_UQC, totals.quantity),
        HsnSacKind::Sac => (SERVICES_UQC, BigDecimal::zero()),
    };
    let breakdown = totals.breakdown;
    HsnRow {
        number,
        hsn_sac,
        description,
        uqc: uqc.to_string(),
        quantity,
        total_value: breakdown.total,
        taxable_value: breakdown.taxable_value,
        igst: breakdown.igst,
        cgst: breakdown.cgst,
        sgst: breakdown.sgst,
        cess: BigDecimal::zero(),
        rate,
    }
}

/// Table 13: the invoices as one series, from the first to the last by date then number
fn doc_issue_section(priced: &[PricedInvoice]) -> DocIssue {
    let (Some(first), Some(last)) = (priced.first(), priced.last()) else {
        return DocIssue::default();
    };
    DocIssue {
        documents: vec![DocSummary {
            doc_num: OUTWARD_INVOICES_DOC_NUM,
            doc_type: OUTWARD_INVOICES_DOC_TYPE.to_string(),
            series: vec![DocSeries {
                number: 1,
                from: first.invoice.invoice_number.clone(),
                to: last.invoice.invoice_number.clone(),
                total: priced.len(),
                cancelled: 0,
                net_issued: priced.len(),
            }],
        }],
    }
}

/// An amount rounded to paise, written as a JSON number
fn portal_amount<S: Serializer>(amount: &BigDecimal, serializer: S) -> Result<S::Ok, S::Error> {
    portal_number(&round_to_paise(amount), serializer)
}

/// A decimal written as a JSON number
fn portal_number<S: Serializer>(value: &BigDecimal, serializer: S) -> Result<S::Ok, S::Error> {
    let number = value
        .to_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| S::Error::custom(format!("{value} does not fit a JSON number")))?;
    serializer.serialize_f64(number)
}

/// A date written `dd-mm-yyyy`
fn portal_date<S: Serializer>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&date.format(PRINT_DATE_FORMAT))
}

/// A flag written `Y` or `N`
fn yes_no<S: Serializer>(flag: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(if *flag { "Y" } else { "N" })
}
