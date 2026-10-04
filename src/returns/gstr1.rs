//! GSTR-1: the return of outward supplies
//!
//! [`Gstr1Return::build`] aggregates a filer's invoices and credit notes for one [`ReturnPeriod`]
//! into the
//! sections of GSTR-1 this crate can fill: Table 4A (B2B supplies, grouped by buyer GSTIN with
//! one item per rate), Table 5 (B2CL: large inter-state supplies to unregistered buyers,
//! grouped by place of supply), Table 7 (B2CS: every other supply to unregistered buyers,
//! summed per place of supply and rate), Table 8 (nil-rated, exempt and non-GST lines, by
//! supply type), Table 9B (credit notes, see [`super::cdn`]), Table 12 (the HSN/SAC summary,
//! grouped by code and rate) and Table 13 (documents issued).
//! The builder is pure; [`Gstr1Return::to_json`] writes the result in the GST portal's
//! offline-tool schema, with its short keys and amounts as JSON numbers rounded to paise, ready
//! to upload.
//!
//! Table 12 is split into the two tabs the portal has used since 2025: rows for B2B supplies go
//! under `hsn.hsn_b2b`, and rows for B2CL and B2CS supplies under `hsn.hsn_b2c`. Which table an
//! invoice belongs to is [`GstInvoice::supply_kind`]. Table 7 reports only taxable supplies other
//! than through an e-commerce operator (`typ` `OE`).
//!
//! A line that charges no GST ([`GstLineItem::charges_gst`]) goes to Table 8, never to Table
//! 4A, 5 or 7, and stays in the HSN summary at rate 0. Its [`SupplyTreatment`] picks the
//! column: a taxable line at 0% is nil-rated, the others are exempt or non-GST. A B2B or B2CL
//! invoice keeps its whole value but lists only its taxed rates, and one with no taxed line
//! appears only in Table 8.
//!
//! A [`CreditNote`] is reported under the supply kind of the invoice it corrects. Against a B2B
//! or B2CL invoice it goes to Table 9B; against a B2CS invoice it is subtracted from the Table 7
//! row for its place of supply and rate, which may then be negative. Either way its lines are
//! subtracted from Table 8 and from the HSN summary tab of its supply kind, and Table 13 lists the
//! notes as a series of their own.

use super::cdn::{cdnr_section, cdnur_section, CdnrParty, CdnurNote};
use super::period::ReturnPeriod;
use crate::invoice::print::PRINT_DATE_FORMAT;
use crate::invoice::types::supply_kind_for;
use crate::invoice::validation::{compliance_errors, credit_note_errors};
use crate::invoice::{
    ComplianceIssue, CreditNote, GstBreakdown, GstDocument, GstInvoice, GstLineItem, Gstin,
    HsnMaster, HsnSacKind, InvoiceError, SupplyKind, SupplyTreatment,
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

/// Table 13 name for credit notes
pub const CREDIT_NOTES_DOC_TYPE: &str = "Credit Note";

/// Table 13 serial number for credit notes
const CREDIT_NOTES_DOC_NUM: usize = 5;

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
    /// Table 5: large inter-state invoices to unregistered buyers, one entry per place of supply
    #[serde(rename = "b2cl", skip_serializing_if = "Vec::is_empty")]
    pub b2cl: Vec<B2clPlace>,
    /// Table 7: other supplies to unregistered buyers, one row per place of supply and rate
    #[serde(rename = "b2cs", skip_serializing_if = "Vec::is_empty")]
    pub b2cs: Vec<B2csRow>,
    /// Table 8: nil-rated, exempt and non-GST supplies, by supply type
    #[serde(rename = "nil", skip_serializing_if = "NilSupplies::is_empty")]
    pub nil: NilSupplies,
    /// Table 9B: credit notes to registered buyers, one entry per buyer GSTIN
    #[serde(rename = "cdnr", skip_serializing_if = "Vec::is_empty")]
    pub cdnr: Vec<CdnrParty>,
    /// Table 9B: credit notes against B2CL invoices to unregistered buyers
    #[serde(rename = "cdnur", skip_serializing_if = "Vec::is_empty")]
    pub cdnur: Vec<CdnurNote>,
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
    pub(super) fn new(rate: BigDecimal, breakdown: &GstBreakdown) -> Self {
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

/// Every B2CL invoice to one place of supply
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2clPlace {
    /// Place of supply: a state code
    #[serde(rename = "pos")]
    pub place_of_supply: String,
    /// The invoices, by date then number
    #[serde(rename = "inv")]
    pub invoices: Vec<B2clInvoice>,
}

/// One invoice in Table 5
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2clInvoice {
    /// Invoice number
    #[serde(rename = "inum")]
    pub invoice_number: String,
    /// Date of issue, written `dd-mm-yyyy`
    #[serde(rename = "idt", serialize_with = "portal_date")]
    pub invoice_date: NaiveDate,
    /// Invoice value: taxable value plus tax
    #[serde(rename = "val", serialize_with = "portal_amount")]
    pub invoice_value: BigDecimal,
    /// One item per GST rate on the invoice, by rate
    #[serde(rename = "itms")]
    pub items: Vec<B2clItem>,
}

/// The lines of one B2CL invoice at one GST rate
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2clItem {
    /// Serial number within the invoice, from 1
    #[serde(rename = "num")]
    pub number: usize,
    /// Taxable value and tax at this rate
    #[serde(rename = "itm_det")]
    pub detail: B2clItemDetail,
}

/// Taxable value and tax of the lines at one rate; a B2CL supply is inter-state, so IGST only
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2clItemDetail {
    /// Total GST rate as a percentage
    #[serde(rename = "rt", serialize_with = "portal_number")]
    pub rate: BigDecimal,
    /// Taxable value
    #[serde(rename = "txval", serialize_with = "portal_amount")]
    pub taxable_value: BigDecimal,
    /// Integrated GST
    #[serde(rename = "iamt", serialize_with = "portal_amount")]
    pub igst: BigDecimal,
    /// Compensation cess, which this crate does not charge
    #[serde(rename = "csamt", serialize_with = "portal_amount")]
    pub cess: BigDecimal,
}

impl B2clItemDetail {
    pub(super) fn new(rate: BigDecimal, breakdown: &GstBreakdown) -> Self {
        Self {
            rate,
            taxable_value: breakdown.taxable_value.clone(),
            igst: breakdown.igst.clone(),
            cess: BigDecimal::zero(),
        }
    }
}

/// Every B2CS line with one place of supply and rate, summed
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct B2csRow {
    /// Whether the supply is within the seller's state or to another
    #[serde(rename = "sply_ty")]
    pub supply_type: SupplyType,
    /// Total GST rate as a percentage
    #[serde(rename = "rt", serialize_with = "portal_number")]
    pub rate: BigDecimal,
    /// How the supply was made
    #[serde(rename = "typ")]
    pub b2cs_type: B2csType,
    /// Place of supply: a state code
    #[serde(rename = "pos")]
    pub place_of_supply: String,
    /// Taxable value
    #[serde(rename = "txval", serialize_with = "portal_amount")]
    pub taxable_value: BigDecimal,
    /// Integrated GST, for an inter-state supply only
    #[serde(
        rename = "iamt",
        serialize_with = "portal_optional_amount",
        skip_serializing_if = "Option::is_none"
    )]
    pub igst: Option<BigDecimal>,
    /// Central GST, for an intra-state supply only
    #[serde(
        rename = "camt",
        serialize_with = "portal_optional_amount",
        skip_serializing_if = "Option::is_none"
    )]
    pub cgst: Option<BigDecimal>,
    /// State GST, for an intra-state supply only
    #[serde(
        rename = "samt",
        serialize_with = "portal_optional_amount",
        skip_serializing_if = "Option::is_none"
    )]
    pub sgst: Option<BigDecimal>,
    /// Compensation cess, which this crate does not charge
    #[serde(rename = "csamt", serialize_with = "portal_amount")]
    pub cess: BigDecimal,
}

impl B2csRow {
    fn new(place_of_supply: &str, is_inter_state: bool, rate: BigDecimal, b: GstBreakdown) -> Self {
        let (supply_type, igst, cgst, sgst) = if is_inter_state {
            (SupplyType::Inter, Some(b.igst), None, None)
        } else {
            (SupplyType::Intra, None, Some(b.cgst), Some(b.sgst))
        };
        Self {
            supply_type,
            rate,
            b2cs_type: B2csType::OtherThanEcommerce,
            place_of_supply: place_of_supply.to_string(),
            taxable_value: b.taxable_value,
            igst,
            cgst,
            sgst,
            cess: BigDecimal::zero(),
        }
    }
}

/// Whether a supply stays in the seller's state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SupplyType {
    /// Within the seller's state: CGST + SGST
    #[serde(rename = "INTRA")]
    Intra,
    /// To another state: IGST
    #[serde(rename = "INTER")]
    Inter,
}

/// How a B2CS supply was made
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum B2csType {
    /// Directly by the filer, not through an e-commerce operator
    #[serde(rename = "OE")]
    OtherThanEcommerce,
}

/// Table 8: nil-rated, exempt and non-GST supplies
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NilSupplies {
    /// One row per supply type
    #[serde(rename = "inv")]
    pub rows: Vec<NilRow>,
}

impl NilSupplies {
    /// Whether no nil-rated supply was made
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The nil-rated, exempt and non-GST supplies of one supply type
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NilRow {
    /// Inter- or intra-state, to registered or unregistered buyers
    #[serde(rename = "sply_ty")]
    pub supply_type: NilSupplyType,
    /// Value of nil-rated supplies: taxable lines at 0%
    #[serde(rename = "nil_amt", serialize_with = "portal_amount")]
    pub nil_rated: BigDecimal,
    /// Value of exempt supplies
    #[serde(rename = "expt_amt", serialize_with = "portal_amount")]
    pub exempt: BigDecimal,
    /// Value of non-GST supplies
    #[serde(rename = "ngsup_amt", serialize_with = "portal_amount")]
    pub non_gst: BigDecimal,
}

impl NilRow {
    /// A row of the supply type with every column at 0
    fn empty(supply_type: NilSupplyType) -> Self {
        Self {
            supply_type,
            nil_rated: BigDecimal::zero(),
            exempt: BigDecimal::zero(),
            non_gst: BigDecimal::zero(),
        }
    }

    /// The column a line of this treatment is added to
    fn column_mut(&mut self, treatment: SupplyTreatment) -> &mut BigDecimal {
        match treatment {
            SupplyTreatment::Taxable => &mut self.nil_rated,
            SupplyTreatment::Exempt => &mut self.exempt,
            SupplyTreatment::NonGst => &mut self.non_gst,
        }
    }
}

/// Supply type of a Table 8 row
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum NilSupplyType {
    /// Inter-state, to registered buyers
    #[serde(rename = "INTRB2B")]
    InterB2b,
    /// Intra-state, to registered buyers
    #[serde(rename = "INTRAB2B")]
    IntraB2b,
    /// Inter-state, to unregistered buyers
    #[serde(rename = "INTRB2C")]
    InterB2c,
    /// Intra-state, to unregistered buyers
    #[serde(rename = "INTRAB2C")]
    IntraB2c,
}

/// Table 12: supplies summarised by HSN/SAC code and rate
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HsnSummary {
    /// Rows for supplies to registered buyers, by code then rate
    #[serde(rename = "hsn_b2b", skip_serializing_if = "Vec::is_empty")]
    pub b2b: Vec<HsnRow>,
    /// Rows for supplies to unregistered buyers (B2CL and B2CS), by code then rate
    #[serde(rename = "hsn_b2c", skip_serializing_if = "Vec::is_empty")]
    pub b2c: Vec<HsnRow>,
}

impl HsnSummary {
    /// Whether the summary has no rows
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.b2b.is_empty() && self.b2c.is_empty()
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
    /// A credit note can't go in the return
    #[error("credit note {note_number} {reason}")]
    InvalidCreditNote {
        /// The note number
        note_number: String,
        /// The first rule it breaks
        reason: DocumentProblem,
    },
    /// An invoice's tax could not be computed
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
    /// The return could not be written as JSON
    #[error("could not write GSTR-1 JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl Gstr1Error {
    /// The error for the invoice numbered `invoice_number` that has `problem`
    fn for_invoice(invoice_number: String, problem: DocumentProblem) -> Self {
        match problem {
            DocumentProblem::SellerMismatch(seller) => Self::SellerMismatch {
                invoice_number,
                seller,
            },
            DocumentProblem::OutsidePeriod { date, period } => Self::OutsidePeriod {
                invoice_number,
                date,
                period,
            },
            DocumentProblem::DuplicateNumber => Self::DuplicateInvoiceNumber(invoice_number),
            DocumentProblem::NotCompliant(issues) => Self::NotCompliant {
                invoice_number,
                issues,
            },
        }
    }
}

/// Why an invoice or credit note can't go in the return
#[derive(Debug, thiserror::Error)]
pub enum DocumentProblem {
    /// It was issued by someone other than the filer
    #[error("was issued by {0}, not the filer")]
    SellerMismatch(Gstin),
    /// It is dated outside the return period
    #[error("dated {date} is outside the period {period}")]
    OutsidePeriod {
        /// Its date
        date: NaiveDate,
        /// The period being filed
        period: ReturnPeriod,
    },
    /// Another document of its kind has the same number, compared case-insensitively
    #[error("appears more than once")]
    DuplicateNumber,
    /// It breaks an error-severity compliance rule; carries the issues
    #[error("is not compliant: {} error(s)", .0.len())]
    NotCompliant(Vec<ComplianceIssue>),
}

/// A document with the tax breakdown of each of its lines, their sum and the supply kind it is
/// reported under, computed once
///
/// The amounts are signed by their effect on the period's supplies: a credit note's are negative,
/// so summing invoices and notes together nets them.
pub(super) struct PricedDocument<'a> {
    pub(super) document: &'a dyn GstDocument,
    pub(super) lines: Vec<GstBreakdown>,
    pub(super) total: GstBreakdown,
    pub(super) kind: SupplyKind,
    reduces_supply: bool,
}

impl<'a> PricedDocument<'a> {
    /// Price an invoice's lines and classify it by supply kind
    fn invoice(invoice: &'a GstInvoice) -> Result<Self, InvoiceError> {
        let lines = invoice.line_breakdowns()?;
        let total: GstBreakdown = lines.iter().sum();
        Ok(Self {
            document: invoice,
            kind: supply_kind_for(invoice, &total.total),
            lines,
            total,
            reduces_supply: false,
        })
    }

    /// Price a credit note's lines as negative amounts, under the supply kind of its invoice
    fn credit_note(note: &'a CreditNote) -> Result<Self, InvoiceError> {
        let lines: Vec<_> = note
            .line_breakdowns()?
            .iter()
            .map(GstBreakdown::negated)
            .collect();
        Ok(Self {
            document: note,
            total: lines.iter().sum(),
            lines,
            kind: note.original.kind,
            reduces_supply: true,
        })
    }

    /// A line's quantity, signed like its amounts
    fn quantity(&self, item: &GstLineItem) -> BigDecimal {
        if self.reduces_supply {
            -&item.quantity
        } else {
            item.quantity.clone()
        }
    }
}

impl Gstr1Return {
    /// Aggregate a filer's invoices and credit notes for a period into GSTR-1
    ///
    /// Every invoice must be issued by `filer`, dated in `period`, carry a number no other
    /// invoice has (ignoring case), pass the error-severity rules of
    /// [`validate_invoice`](crate::invoice::validate_invoice) as of its own date.
    /// [`GstInvoice::supply_kind`] picks its table. Every credit note meets the same rules, with
    /// [`validate_credit_note`](crate::invoice::validate_credit_note), its number unique among the
    /// notes; the supply kind of its invoice picks its table. `master` supplies the HSN/SAC summary's
    /// descriptions, usually [`HsnMaster::global`]. Output is ordered by buyer GSTIN or place of
    /// supply, then invoice date and number, then rate, whatever the input order.
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::SellerMismatch`], [`Gstr1Error::OutsidePeriod`],
    /// [`Gstr1Error::DuplicateInvoiceNumber`] or [`Gstr1Error::NotCompliant`] for the first
    /// invoice that fails, [`Gstr1Error::InvalidCreditNote`] for the first credit note that fails,
    /// or [`Gstr1Error::Invoice`] if a line's tax can't be computed.
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
    /// let gstr1 = Gstr1Return::build(&seller, period, &[invoice], &[], HsnMaster::global())?;
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
        credit_notes: &[CreditNote],
        master: &HsnMaster,
    ) -> Result<Self, Gstr1Error> {
        check_documents(filer, period, invoices, |invoice| {
            compliance_errors(invoice, invoice.invoice_date)
        })
        .map_err(|(number, problem)| Gstr1Error::for_invoice(number, problem))?;
        check_documents(filer, period, credit_notes, |note| {
            credit_note_errors(note, note.note_date)
        })
        .map_err(|(note_number, reason)| Gstr1Error::InvalidCreditNote {
            note_number,
            reason,
        })?;

        let invoices = priced(invoices, PricedDocument::invoice)?;
        let notes = priced(credit_notes, PricedDocument::credit_note)?;
        Ok(Self::from_priced(filer, period, &invoices, &notes, master))
    }

    /// Lay the checked, priced and ordered documents out in the return's tables
    fn from_priced(
        filer: &Gstin,
        period: ReturnPeriod,
        invoices: &[PricedDocument],
        notes: &[PricedDocument],
        master: &HsnMaster,
    ) -> Self {
        let all = || invoices.iter().chain(notes);
        let b2c_kinds = [SupplyKind::B2cl, SupplyKind::B2cs];
        Self {
            filer_gstin: filer.clone(),
            period,
            b2b: b2b_section(&of_kind(invoices, &[SupplyKind::B2b])),
            b2cl: b2cl_section(&of_kind(invoices, &[SupplyKind::B2cl])),
            b2cs: b2cs_section(&of_kind(all(), &[SupplyKind::B2cs])),
            nil: nil_section(all()),
            cdnr: cdnr_section(&of_kind(notes, &[SupplyKind::B2b])),
            cdnur: cdnur_section(&of_kind(notes, &[SupplyKind::B2cl])),
            hsn: HsnSummary {
                b2b: hsn_rows(&of_kind(all(), &[SupplyKind::B2b]), master),
                b2c: hsn_rows(&of_kind(all(), &b2c_kinds), master),
            },
            doc_issue: DocIssue {
                documents: [
                    doc_summary(
                        OUTWARD_INVOICES_DOC_NUM,
                        OUTWARD_INVOICES_DOC_TYPE,
                        invoices,
                    ),
                    doc_summary(CREDIT_NOTES_DOC_NUM, CREDIT_NOTES_DOC_TYPE, notes),
                ]
                .into_iter()
                .flatten()
                .collect(),
            },
        }
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

/// Documents by date, then number with its trailing digits compared as a number, so that
/// `INV-9` comes before `INV-10`
fn document_order(a: &dyn GstDocument, b: &dyn GstDocument) -> Ordering {
    (a.date(), number_key(a.number()))
        .cmp(&(b.date(), number_key(b.number())))
        .then_with(|| a.number().cmp(b.number()))
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

/// Check every document, and that no two share a number ignoring case; the first failure comes
/// back with the number of the document that failed
fn check_documents<D: GstDocument>(
    filer: &Gstin,
    period: ReturnPeriod,
    documents: &[D],
    errors: impl Fn(&D) -> Vec<ComplianceIssue>,
) -> Result<(), (String, DocumentProblem)> {
    documents
        .iter()
        .try_fold(HashSet::new(), |mut seen, document| {
            let problem = document_problem(filer, period, document, &errors).or_else(|| {
                (!seen.insert(document.number().to_ascii_uppercase()))
                    .then_some(DocumentProblem::DuplicateNumber)
            });
            match problem {
                Some(problem) => Err((document.number().to_string(), problem)),
                None => Ok(seen),
            }
        })
        .map(drop)
}

/// The first rule one document breaks on its own, if any
fn document_problem<D: GstDocument>(
    filer: &Gstin,
    period: ReturnPeriod,
    document: &D,
    errors: impl Fn(&D) -> Vec<ComplianceIssue>,
) -> Option<DocumentProblem> {
    if document.seller_gstin() != filer {
        return Some(DocumentProblem::SellerMismatch(
            document.seller_gstin().clone(),
        ));
    }
    if !period.contains(document.date()) {
        return Some(DocumentProblem::OutsidePeriod {
            date: document.date(),
            period,
        });
    }
    let issues = errors(document);
    (!issues.is_empty()).then_some(DocumentProblem::NotCompliant(issues))
}

/// Every document priced with `price`, by date then number
fn priced<'a, D>(
    documents: &'a [D],
    price: impl Fn(&'a D) -> Result<PricedDocument<'a>, InvoiceError>,
) -> Result<Vec<PricedDocument<'a>>, InvoiceError> {
    let mut priced = documents.iter().map(price).collect::<Result<Vec<_>, _>>()?;
    priced.sort_by(|a, b| document_order(a.document, b.document));
    Ok(priced)
}

/// The priced documents of the given supply kinds, keeping their order
fn of_kind<'p, 'a: 'p>(
    priced: impl IntoIterator<Item = &'p PricedDocument<'a>>,
    kinds: &[SupplyKind],
) -> Vec<&'p PricedDocument<'a>> {
    priced
        .into_iter()
        .filter(|p| kinds.contains(&p.kind))
        .collect()
}

/// Values grouped under their keys, in key order, keeping their order within each key
pub(super) fn grouped<K: Ord, V>(pairs: impl Iterator<Item = (K, V)>) -> BTreeMap<K, Vec<V>> {
    pairs.fold(BTreeMap::new(), |mut groups, (key, value)| {
        groups.entry(key).or_insert_with(Vec::new).push(value);
        groups
    })
}

/// Table 4A: invoices grouped by buyer GSTIN, keeping their order within each buyer
///
/// An invoice with no taxed line is left out: its lines are all in Table 8.
fn b2b_section(priced: &[&PricedDocument]) -> Vec<B2bParty> {
    grouped(
        priced
            .iter()
            .filter_map(|p| Some((p.document.buyer().gstin()?, b2b_invoice(p)?))),
    )
    .into_iter()
    .map(|(buyer_gstin, invoices)| B2bParty {
        buyer_gstin: buyer_gstin.clone(),
        invoices,
    })
    .collect()
}

/// One Table 4A invoice with an item per taxed rate, or `None` when no line charges GST; its
/// value stays the whole invoice's
fn b2b_invoice(p: &PricedDocument) -> Option<B2bInvoice> {
    let items: Vec<_> = taxable_rates(p)
        .enumerate()
        .map(|(i, (rate, breakdown))| B2bItem {
            number: i + 1,
            detail: ItemDetail::new(rate, &breakdown),
        })
        .collect();
    (!items.is_empty()).then(|| B2bInvoice {
        invoice_number: p.document.number().to_string(),
        invoice_date: p.document.date(),
        invoice_value: p.total.total.clone(),
        place_of_supply: p.document.buyer().place_of_supply().to_string(),
        reverse_charge: false,
        invoice_type: B2bInvoiceType::Regular,
        items,
    })
}

/// Table 5: invoices grouped by place of supply, keeping their order within each place
///
/// An invoice with no taxable line is left out: its 0% lines are all in Table 8.
fn b2cl_section(priced: &[&PricedDocument]) -> Vec<B2clPlace> {
    grouped(
        priced
            .iter()
            .filter_map(|p| Some((p.document.buyer().place_of_supply(), b2cl_invoice(p)?))),
    )
    .into_iter()
    .map(|(place_of_supply, invoices)| B2clPlace {
        place_of_supply: place_of_supply.to_string(),
        invoices,
    })
    .collect()
}

/// One Table 5 invoice with an item per taxed rate, or `None` when no line charges GST; its
/// value stays the whole invoice's, untaxed lines included
fn b2cl_invoice(p: &PricedDocument) -> Option<B2clInvoice> {
    let items: Vec<_> = taxable_rates(p)
        .enumerate()
        .map(|(i, (rate, breakdown))| B2clItem {
            number: i + 1,
            detail: B2clItemDetail::new(rate, &breakdown),
        })
        .collect();
    (!items.is_empty()).then(|| B2clInvoice {
        invoice_number: p.document.number().to_string(),
        invoice_date: p.document.date(),
        invoice_value: p.total.total.clone(),
        items,
    })
}

/// Table 7: every taxed line summed per place of supply and rate; untaxed lines go to Table 8
///
/// The filer is the seller of every invoice, so one place of supply is either always inside its
/// state or always outside it, and the supply type is the same for every line of a row.
fn b2cs_section(priced: &[&PricedDocument]) -> Vec<B2csRow> {
    priced
        .iter()
        .flat_map(|p| {
            let key = (
                p.document.buyer().place_of_supply(),
                p.document.is_inter_state(),
            );
            taxable_rates(p).map(move |(rate, b)| (key, rate, b))
        })
        .fold(
            BTreeMap::new(),
            |mut rows, ((pos, inter), rate, breakdown)| {
                *rows
                    .entry((pos, rate, inter))
                    .or_insert_with(GstBreakdown::default) += &breakdown;
                rows
            },
        )
        .into_iter()
        .map(|((pos, rate, inter), breakdown)| B2csRow::new(pos, inter, rate, breakdown))
        .collect()
}

/// Table 8: every line that charges no GST, summed per supply type into the column of its
/// treatment
fn nil_section<'p, 'a: 'p>(priced: impl Iterator<Item = &'p PricedDocument<'a>>) -> NilSupplies {
    let rows = priced
        .flat_map(|p| {
            let supply_type = nil_supply_type(p.document);
            p.document
                .line_items()
                .iter()
                .zip(&p.lines)
                .filter(|(item, _)| !item.charges_gst())
                .map(move |(item, b)| (supply_type, item.treatment, &b.taxable_value))
        })
        .fold(
            BTreeMap::new(),
            |mut rows, (supply_type, treatment, value)| {
                *rows
                    .entry(supply_type)
                    .or_insert_with(|| NilRow::empty(supply_type))
                    .column_mut(treatment) += value;
                rows
            },
        );
    NilSupplies {
        rows: rows.into_values().collect(),
    }
}

/// The Table 8 row an invoice's untaxed lines belong to
fn nil_supply_type(document: &dyn GstDocument) -> NilSupplyType {
    match (document.buyer().is_registered(), document.is_inter_state()) {
        (true, true) => NilSupplyType::InterB2b,
        (true, false) => NilSupplyType::IntraB2b,
        (false, true) => NilSupplyType::InterB2c,
        (false, false) => NilSupplyType::IntraB2c,
    }
}

/// An invoice's breakdowns per rate over the lines that charge GST: what Tables 4A, 5 and 7
/// report, Table 8 taking the rest (the same split [`nil_section`] makes)
pub(super) fn taxable_rates(
    p: &PricedDocument,
) -> impl Iterator<Item = (BigDecimal, GstBreakdown)> {
    by_rate_where(p, GstLineItem::charges_gst).into_iter()
}

/// The breakdowns of the lines `keep` accepts, summed per rate so that 18 and 18.00 are one rate
fn by_rate_where(
    p: &PricedDocument,
    keep: impl Fn(&GstLineItem) -> bool,
) -> BTreeMap<BigDecimal, GstBreakdown> {
    p.document
        .line_items()
        .iter()
        .zip(&p.lines)
        .filter(|(item, _)| keep(item))
        .fold(BTreeMap::new(), |mut rates, (item, breakdown)| {
            *rates.entry(item.gst_rate.normalized()).or_default() += breakdown;
            rates
        })
}

/// The lines of every invoice with one HSN/SAC code and rate, before numbering
#[derive(Default)]
struct HsnTotals {
    quantity: BigDecimal,
    breakdown: GstBreakdown,
    first_description: String,
}

/// One tab of Table 12: every line of `priced` grouped by HSN/SAC code and rate
fn hsn_rows(priced: &[&PricedDocument], master: &HsnMaster) -> Vec<HsnRow> {
    let totals = priced
        .iter()
        .flat_map(|p| {
            p.document
                .line_items()
                .iter()
                .zip(&p.lines)
                .map(|(item, breakdown)| (item, breakdown, p.quantity(item)))
        })
        .fold(BTreeMap::new(), |mut rows, (item, breakdown, quantity)| {
            let totals: &mut HsnTotals = rows
                .entry((item.hsn_sac.clone(), item.gst_rate.normalized()))
                .or_default();
            if totals.first_description.is_empty() {
                totals.first_description.clone_from(&item.description);
            }
            totals.quantity += quantity;
            totals.breakdown += breakdown;
            rows
        });

    totals
        .into_iter()
        .enumerate()
        .map(|(i, ((hsn_sac, rate), totals))| hsn_row(i + 1, hsn_sac, rate, totals, master))
        .collect()
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

/// One kind of document in Table 13: the documents as one series, from the first to the last by
/// date then number, or `None` when none were issued
fn doc_summary(doc_num: usize, doc_type: &str, priced: &[PricedDocument]) -> Option<DocSummary> {
    let (first, last) = (priced.first()?, priced.last()?);
    Some(DocSummary {
        doc_num,
        doc_type: doc_type.to_string(),
        series: vec![DocSeries {
            number: 1,
            from: first.document.number().to_string(),
            to: last.document.number().to_string(),
            total: priced.len(),
            cancelled: 0,
            net_issued: priced.len(),
        }],
    })
}

/// An amount rounded to paise, written as a JSON number
pub(super) fn portal_amount<S: Serializer>(
    amount: &BigDecimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    portal_number(&round_to_paise(amount), serializer)
}

/// An amount that is present, rounded to paise and written as a JSON number
#[allow(clippy::ref_option)] // serde's `serialize_with` passes a `&Option<_>`
fn portal_optional_amount<S: Serializer>(
    amount: &Option<BigDecimal>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match amount {
        Some(amount) => portal_amount(amount, serializer),
        None => serializer.serialize_none(),
    }
}

/// A decimal written as a JSON number
pub(super) fn portal_number<S: Serializer>(
    value: &BigDecimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let number = value
        .to_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| S::Error::custom(format!("{value} does not fit a JSON number")))?;
    serializer.serialize_f64(number)
}

/// A date written `dd-mm-yyyy`
pub(super) fn portal_date<S: Serializer>(
    date: &NaiveDate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&date.format(PRINT_DATE_FORMAT))
}

/// A flag written `Y` or `N`
pub(super) fn yes_no<S: Serializer>(flag: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(if *flag { "Y" } else { "N" })
}
