//! GSTR-1 Table 9B: credit notes
//!
//! A credit note is reported by the supply kind of the invoice it corrects. Notes against B2B
//! invoices go to `cdnr`, grouped by buyer GSTIN like Table 4A. Notes against B2CL invoices go to
//! `cdnur` with the type `B2CL`. Notes against B2CS invoices have no Table 9B entry: the GSTR-1
//! builder subtracts them from the Table 7 rows instead.
//!
//! The amounts here are positive, as the portal expects; the note type `C` says they reduce the
//! supply. Like an invoice, a note lists only the rates it charges GST at, keeps its whole value,
//! and is left out when no line charges GST, its lines then being netted in Table 8 alone.

use super::gstr1::{
    grouped, portal_amount, portal_date, taxable_rates, yes_no, B2bInvoiceType, B2bItem, B2clItem,
    B2clItemDetail, ItemDetail, PricedDocument,
};
use crate::invoice::{GstBreakdown, Gstin};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::Serialize;

/// Every credit note to one registered buyer
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CdnrParty {
    /// Buyer's GSTIN
    #[serde(rename = "ctin")]
    pub buyer_gstin: Gstin,
    /// The buyer's notes, by date then number
    #[serde(rename = "nt")]
    pub notes: Vec<CdnrNote>,
}

/// One credit note to a registered buyer
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CdnrNote {
    /// Kind of note
    #[serde(rename = "ntty")]
    pub note_type: NoteType,
    /// Note number
    #[serde(rename = "nt_num")]
    pub note_number: String,
    /// Date of issue, written `dd-mm-yyyy`
    #[serde(rename = "nt_dt", serialize_with = "portal_date")]
    pub note_date: NaiveDate,
    /// Place of supply: the buyer's state code
    #[serde(rename = "pos")]
    pub place_of_supply: String,
    /// Whether tax is payable on reverse charge, written `Y` or `N`
    #[serde(rename = "rchrg", serialize_with = "yes_no")]
    pub reverse_charge: bool,
    /// Kind of the invoice the note corrects
    #[serde(rename = "inv_typ")]
    pub invoice_type: B2bInvoiceType,
    /// Note value: taxable value plus tax
    #[serde(rename = "val", serialize_with = "portal_amount")]
    pub note_value: BigDecimal,
    /// One item per GST rate on the note, by rate
    #[serde(rename = "itms")]
    pub items: Vec<B2bItem>,
}

/// One credit note to an unregistered buyer, against a B2CL invoice
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CdnurNote {
    /// Kind of the invoice the note corrects
    #[serde(rename = "typ")]
    pub original_type: CdnurType,
    /// Kind of note
    #[serde(rename = "ntty")]
    pub note_type: NoteType,
    /// Note number
    #[serde(rename = "nt_num")]
    pub note_number: String,
    /// Date of issue, written `dd-mm-yyyy`
    #[serde(rename = "nt_dt", serialize_with = "portal_date")]
    pub note_date: NaiveDate,
    /// Place of supply: a state code
    #[serde(rename = "pos")]
    pub place_of_supply: String,
    /// Note value: taxable value plus tax
    #[serde(rename = "val", serialize_with = "portal_amount")]
    pub note_value: BigDecimal,
    /// One item per GST rate on the note, by rate; inter-state, so IGST only
    #[serde(rename = "itms")]
    pub items: Vec<B2clItem>,
}

/// Kind of a Table 9B note
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum NoteType {
    /// A credit note, which reduces the supply
    #[serde(rename = "C")]
    Credit,
}

/// Kind of invoice a `cdnur` note corrects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum CdnurType {
    /// A large inter-state supply to an unregistered buyer (Table 5)
    #[serde(rename = "B2CL")]
    B2cl,
}

/// Table 9B `cdnr`: notes grouped by buyer GSTIN, keeping their order within each buyer
pub(super) fn cdnr_section(notes: &[&PricedDocument]) -> Vec<CdnrParty> {
    grouped(
        notes
            .iter()
            .filter_map(|p| Some((p.document.buyer().gstin()?, cdnr_note(p)?))),
    )
    .into_iter()
    .map(|(buyer_gstin, notes)| CdnrParty {
        buyer_gstin: buyer_gstin.clone(),
        notes,
    })
    .collect()
}

/// One `cdnr` note with an item per taxed rate, or `None` when no line charges GST
fn cdnr_note(p: &PricedDocument) -> Option<CdnrNote> {
    let items: Vec<_> = issued_rates(p)
        .enumerate()
        .map(|(i, (rate, breakdown))| B2bItem {
            number: i + 1,
            detail: ItemDetail::new(rate, &breakdown),
        })
        .collect();
    (!items.is_empty()).then(|| CdnrNote {
        note_type: NoteType::Credit,
        note_number: p.document.number().to_string(),
        note_date: p.document.date(),
        place_of_supply: p.document.buyer().place_of_supply().to_string(),
        reverse_charge: false,
        invoice_type: B2bInvoiceType::Regular,
        note_value: -&p.total.total,
        items,
    })
}

/// Table 9B `cdnur`: the notes against B2CL invoices, by date then number
pub(super) fn cdnur_section(notes: &[&PricedDocument]) -> Vec<CdnurNote> {
    notes.iter().filter_map(|p| cdnur_note(p)).collect()
}

/// One `cdnur` note with an item per taxed rate, or `None` when no line charges GST
fn cdnur_note(p: &PricedDocument) -> Option<CdnurNote> {
    let items: Vec<_> = issued_rates(p)
        .enumerate()
        .map(|(i, (rate, breakdown))| B2clItem {
            number: i + 1,
            detail: B2clItemDetail::new(rate, &breakdown),
        })
        .collect();
    (!items.is_empty()).then(|| CdnurNote {
        original_type: CdnurType::B2cl,
        note_type: NoteType::Credit,
        note_number: p.document.number().to_string(),
        note_date: p.document.date(),
        place_of_supply: p.document.buyer().place_of_supply().to_string(),
        note_value: -&p.total.total,
        items,
    })
}

/// A note's breakdowns per taxed rate as issued: positive, where the priced note nets them
/// negative
fn issued_rates(p: &PricedDocument) -> impl Iterator<Item = (BigDecimal, GstBreakdown)> {
    taxable_rates(p).map(|(rate, breakdown)| (rate, breakdown.negated()))
}
