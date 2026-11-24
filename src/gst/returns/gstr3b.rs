//! GSTR-3B's outward supplies: Table 3.1 from the period's invoices and credit notes
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
//! A credit note is subtracted from the row its lines fall in. Building is pure, and the
//! documents are checked as [`Gstr1Return::build`](super::Gstr1Return::build) checks them.

use super::gstr1::{checked_and_priced, Gstr1Error, PricedDocument};
use super::period::ReturnPeriod;
use crate::gst::invoice::SupplyTreatment;
use crate::gst::invoice::{GstBreakdown, GstCreditNote, GstInvoice, GstLineItem, Gstin};
use bigdecimal::BigDecimal;
use serde::Serialize;

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

/// GSTR-3B's outward supplies for one filer and period
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gstr3bOutward {
    /// The filer's GSTIN
    pub filer_gstin: Gstin,
    /// The month filed
    pub period: ReturnPeriod,
    /// Table 3.1
    pub outward: OutwardSupplies,
}

impl Gstr3bOutward {
    /// Sum a filer's invoices and credit notes for a period into Table 3.1
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
        })
    }
}
