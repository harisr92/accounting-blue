//! GSTR-3B's outward supplies: Tables 3.1 and 3.2 from the period's invoices and credit notes
//!
//! GSTR-3B is the monthly summary return a regular taxpayer files with the tax paid. Table 3.1
//! reports the month's outward supplies in five rows; this builds the four that sales decide:
//!
//! - (a) taxable supplies other than zero-rated, nil-rated and exempt: the lines that charge GST;
//! - (b) zero-rated supplies (exports and supplies to SEZs): none, since exports aren't covered;
//! - (c) other outward supplies, nil-rated and exempt: taxable lines at 0% and exempt lines;
//! - (d) inward supplies liable to reverse charge: purchases, not built from sales, so zero;
//! - (e) non-GST outward supplies.
//!
//! Table 3.2 breaks the inter-state part of row (a) down by place of supply for supplies to
//! unregistered persons, composition taxable persons and UIN holders. Only the first is built:
//! the crate has no composition or UIN-holder buyers.
//!
//! A credit note is subtracted from the row its lines fall in. Bills of supply, and credit notes
//! against them, are left out: their sellers, under composition or not registered, don't file
//! GSTR-3B. Building is pure, and the
//! documents are checked as [`Gstr1Return::build`](super::Gstr1Return::build) checks them.
//! [`Gstr3bOutward::to_json`] writes the result in the GST portal's GSTR-3B schema for upload.
//! It leaves out the tables sales don't decide: Table 3.1.1 (supplies through an e-commerce
//! operator), Table 4 (input tax credit), Table 5 (exempt inward supplies), Table 5.1 (interest
//! and late fee) and Table 6 (payment of tax).
//!
//! Rows are written as summed. A month whose credit notes outweigh its sales makes a row
//! negative; the portal refuses negative values, so the filer adjusts those figures by hand.

use super::gstr1::{checked_and_priced, Gstr1Error, PricedDocument};
use super::period::ReturnPeriod;
use super::portal::portal_amount;
use crate::gst::invoice::{GstBreakdown, GstCreditNote, GstInvoice, GstLineItem, Gstin};
use crate::gst::invoice::{SupplyKind, SupplyTreatment};
use bigdecimal::BigDecimal;
use serde::Serialize;
use std::collections::BTreeMap;

/// One row of Table 3.1: the value of the supplies and the tax on them
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct OutwardRow {
    /// Taxable value
    pub taxable_value: BigDecimal,
    /// Integrated tax
    pub igst: BigDecimal,
    /// Central tax
    pub cgst: BigDecimal,
    /// State or union territory tax
    pub sgst: BigDecimal,
    /// Compensation cess; not charged by these documents, so always zero
    pub cess: BigDecimal,
}

impl OutwardRow {
    /// The row with `breakdown` added
    fn plus(self, breakdown: &GstBreakdown) -> Self {
        Self {
            taxable_value: self.taxable_value + &breakdown.taxable_value,
            igst: self.igst + &breakdown.igst,
            cgst: self.cgst + &breakdown.cgst,
            sgst: self.sgst + &breakdown.sgst,
            cess: self.cess,
        }
    }
}

/// Table 3.1 of GSTR-3B: the period's outward supplies
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct OutwardSupplies {
    /// (a) Outward taxable supplies (other than zero-rated, nil-rated and exempted)
    pub taxable: OutwardRow,
    /// (b) Outward taxable supplies (zero-rated)
    pub zero_rated: OutwardRow,
    /// (c) Other outward supplies (nil-rated, exempted)
    pub nil_rated_exempt: OutwardRow,
    /// (d) Inward supplies (liable to reverse charge)
    pub reverse_charge: OutwardRow,
    /// (e) Non-GST outward supplies
    pub non_gst: OutwardRow,
}

/// The rows of Table 3.1 a sales line can fall in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutwardKind {
    Taxable,
    NilRatedExempt,
    NonGst,
}

/// The row a line falls in, by whether it charges GST and its treatment
fn outward_kind(item: &GstLineItem) -> OutwardKind {
    match item.treatment {
        SupplyTreatment::NonGst => OutwardKind::NonGst,
        _ if item.charges_gst() => OutwardKind::Taxable,
        SupplyTreatment::Taxable | SupplyTreatment::Exempt => OutwardKind::NilRatedExempt,
    }
}

impl OutwardSupplies {
    fn row_mut(&mut self, kind: OutwardKind) -> &mut OutwardRow {
        match kind {
            OutwardKind::Taxable => &mut self.taxable,
            OutwardKind::NilRatedExempt => &mut self.nil_rated_exempt,
            OutwardKind::NonGst => &mut self.non_gst,
        }
    }

    /// Every line of the priced documents added to its row; a credit note's amounts are negative
    fn from_priced<'p, 'a: 'p>(priced: impl Iterator<Item = &'p PricedDocument<'a>>) -> Self {
        priced
            .flat_map(|p| p.document.line_items().iter().zip(&p.lines))
            .fold(Self::default(), |mut supplies, (item, breakdown)| {
                let row = supplies.row_mut(outward_kind(item));
                *row = std::mem::take(row).plus(breakdown);
                supplies
            })
    }
}

/// One row of Table 3.2: inter-state supplies into one state and the integrated tax on them
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InterStateRow {
    /// Two-digit state code of the place of supply
    pub place_of_supply: String,
    /// Taxable value
    pub taxable_value: BigDecimal,
    /// Integrated tax
    pub igst: BigDecimal,
}

/// Table 3.2's rows for unregistered buyers: the GST-charging lines of inter-state B2CL and B2CS
/// documents, summed per place of supply and ordered by state code; credit notes are negative
fn inter_state_unregistered<'p, 'a: 'p>(
    priced: impl Iterator<Item = &'p PricedDocument<'a>>,
) -> Vec<InterStateRow> {
    priced
        .filter(|p| matches!(p.kind, SupplyKind::B2cl | SupplyKind::B2cs))
        .filter(|p| p.document.is_inter_state())
        .flat_map(|p| {
            let place = p.document.buyer().place_of_supply();
            p.document
                .line_items()
                .iter()
                .zip(&p.lines)
                .filter(|(item, _)| outward_kind(item) == OutwardKind::Taxable)
                .map(move |(_, breakdown)| (place, breakdown))
        })
        .fold(BTreeMap::new(), |mut places, (place, breakdown)| {
            let (taxable, igst): &mut (BigDecimal, BigDecimal) = places.entry(place).or_default();
            *taxable += &breakdown.taxable_value;
            *igst += &breakdown.igst;
            places
        })
        .into_iter()
        .map(|(place, (taxable_value, igst))| InterStateRow {
            place_of_supply: place.to_string(),
            taxable_value,
            igst,
        })
        .collect()
}

/// GSTR-3B's outward supplies for one filer and period
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gstr3bOutward {
    /// The filer's GSTIN
    pub filer_gstin: Gstin,
    /// The month filed
    pub period: ReturnPeriod,
    /// Table 3.1
    pub outward: OutwardSupplies,
    /// Table 3.2: inter-state supplies to unregistered persons, by place of supply
    pub inter_state_unregistered: Vec<InterStateRow>,
}

impl Gstr3bOutward {
    /// Sum a filer's invoices and credit notes for a period into Tables 3.1 and 3.2
    ///
    /// # Errors
    ///
    /// The same as [`Gstr1Return::build`](super::Gstr1Return::build): the documents are checked
    /// the same way, so a month that files as GSTR-1 summarises as GSTR-3B.
    pub fn build(
        filer: &Gstin,
        period: ReturnPeriod,
        invoices: &[GstInvoice],
        credit_notes: &[GstCreditNote],
    ) -> Result<Self, Gstr1Error> {
        let (invoices, notes) = checked_and_priced(filer, period, invoices, credit_notes)?;
        Ok(Self {
            filer_gstin: filer.clone(),
            period,
            outward: OutwardSupplies::from_priced(invoices.iter().chain(&notes)),
            inter_state_unregistered: inter_state_unregistered(invoices.iter().chain(&notes)),
        })
    }

    /// The return as pretty-printed JSON in the GST portal's GSTR-3B schema
    ///
    /// Each row of Table 3.1 carries only the columns the portal takes for it, and amounts are
    /// rounded to paise and written as JSON numbers. Negative rows are written as they are.
    ///
    /// # Errors
    ///
    /// [`Gstr1Error::Json`] if an amount is too large to write as a JSON number.
    pub fn to_json(&self) -> Result<String, Gstr1Error> {
        Ok(serde_json::to_string_pretty(&PortalGstr3b::from(self))?)
    }
}

/// [`Gstr3bOutward`] in the portal's schema, borrowing its values
#[derive(Serialize)]
struct PortalGstr3b<'a> {
    gstin: &'a Gstin,
    ret_period: String,
    sup_details: PortalSupDetails<'a>,
    inter_sup: PortalInterSup<'a>,
}

/// Table 3.1, one key per row
#[derive(Serialize)]
struct PortalSupDetails<'a> {
    osup_det: FullRow<'a>,
    osup_zero: ZeroRatedRow<'a>,
    osup_nil_exmp: ValueRow<'a>,
    isup_rev: FullRow<'a>,
    osup_nongst: ValueRow<'a>,
}

/// A row with every column: rows (a) and (d)
#[derive(Serialize)]
struct FullRow<'a> {
    #[serde(serialize_with = "portal_amount")]
    txval: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    iamt: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    camt: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    samt: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    csamt: &'a BigDecimal,
}

/// Row (b): zero-rated supplies are inter-state, so there is no central or state tax
#[derive(Serialize)]
struct ZeroRatedRow<'a> {
    #[serde(serialize_with = "portal_amount")]
    txval: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    iamt: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    csamt: &'a BigDecimal,
}

/// A row with the value alone: rows (c) and (e), which carry no tax
#[derive(Serialize)]
struct ValueRow<'a> {
    #[serde(serialize_with = "portal_amount")]
    txval: &'a BigDecimal,
}

/// Table 3.2, one list per kind of buyer
#[derive(Serialize)]
struct PortalInterSup<'a> {
    unreg_details: Vec<PortalInterRow<'a>>,
    comp_details: Vec<PortalInterRow<'a>>,
    uin_details: Vec<PortalInterRow<'a>>,
}

/// One place of supply in Table 3.2
#[derive(Serialize)]
struct PortalInterRow<'a> {
    pos: &'a str,
    #[serde(serialize_with = "portal_amount")]
    txval: &'a BigDecimal,
    #[serde(serialize_with = "portal_amount")]
    iamt: &'a BigDecimal,
}

impl<'a> From<&'a OutwardRow> for FullRow<'a> {
    fn from(row: &'a OutwardRow) -> Self {
        Self {
            txval: &row.taxable_value,
            iamt: &row.igst,
            camt: &row.cgst,
            samt: &row.sgst,
            csamt: &row.cess,
        }
    }
}

impl<'a> From<&'a OutwardRow> for ZeroRatedRow<'a> {
    fn from(row: &'a OutwardRow) -> Self {
        Self {
            txval: &row.taxable_value,
            iamt: &row.igst,
            csamt: &row.cess,
        }
    }
}

impl<'a> From<&'a OutwardRow> for ValueRow<'a> {
    fn from(row: &'a OutwardRow) -> Self {
        Self {
            txval: &row.taxable_value,
        }
    }
}

impl<'a> From<&'a InterStateRow> for PortalInterRow<'a> {
    fn from(row: &'a InterStateRow) -> Self {
        Self {
            pos: &row.place_of_supply,
            txval: &row.taxable_value,
            iamt: &row.igst,
        }
    }
}

impl<'a> From<&'a Gstr3bOutward> for PortalGstr3b<'a> {
    fn from(summary: &'a Gstr3bOutward) -> Self {
        let outward = &summary.outward;
        Self {
            gstin: &summary.filer_gstin,
            ret_period: summary.period.to_portal_string(),
            sup_details: PortalSupDetails {
                osup_det: FullRow::from(&outward.taxable),
                osup_zero: ZeroRatedRow::from(&outward.zero_rated),
                osup_nil_exmp: ValueRow::from(&outward.nil_rated_exempt),
                isup_rev: FullRow::from(&outward.reverse_charge),
                osup_nongst: ValueRow::from(&outward.non_gst),
            },
            inter_sup: PortalInterSup {
                unreg_details: summary
                    .inter_state_unregistered
                    .iter()
                    .map(PortalInterRow::from)
                    .collect(),
                comp_details: Vec::new(),
                uin_details: Vec::new(),
            },
        }
    }
}
