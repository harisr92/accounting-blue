//! HSN/SAC master data and default GST rates
//!
//! The master is compiled into the crate from `src/data/hsn_master.json`. It covers common codes
//! for Indian SMEs under the GST schedule named by [`HsnMaster::schedule`], not the full CBIC
//! tariff. Rates are defaults: some goods and services carry conditional rates (price thresholds,
//! input tax credit options), so callers can always pass an explicit rate instead.

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

/// The embedded master, parsed once on first use
static MASTER: LazyLock<HsnMaster> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/hsn_master.json"))
        .expect("embedded HSN/SAC master is valid JSON")
});

/// Whether a code classifies goods or services
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HsnSacKind {
    /// Harmonized System of Nomenclature code (goods)
    Hsn,
    /// Services Accounting Code (services)
    Sac,
}

/// One code in the master
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HsnSacEntry {
    /// HSN or SAC code: 4, 6 or 8 digits
    pub code: String,
    /// Goods or services
    pub kind: HsnSacKind,
    /// Short description of what the code covers
    pub description: String,
    /// Default total GST rate as a percentage (e.g. 18 for 18%)
    pub gst_rate: BigDecimal,
}

/// Lookup table of HSN/SAC codes and their default GST rates
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawMaster")]
pub struct HsnMaster {
    schedule: String,
    effective_from: NaiveDate,
    entries: Vec<HsnSacEntry>,
    index: HashMap<String, usize>,
}

/// On-disk shape of the master, before indexing
#[derive(Deserialize)]
struct RawMaster {
    schedule: String,
    effective_from: NaiveDate,
    entries: Vec<HsnSacEntry>,
}

impl From<RawMaster> for HsnMaster {
    fn from(raw: RawMaster) -> Self {
        let index = raw
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.code.clone(), i))
            .collect();

        Self {
            schedule: raw.schedule,
            effective_from: raw.effective_from,
            entries: raw.entries,
            index,
        }
    }
}

impl HsnMaster {
    /// The master embedded in the crate
    pub fn global() -> &'static HsnMaster {
        &MASTER
    }

    /// Name of the GST rate schedule the default rates follow
    pub fn schedule(&self) -> &str {
        &self.schedule
    }

    /// Date the rate schedule took effect
    pub fn effective_from(&self) -> NaiveDate {
        self.effective_from
    }

    /// All entries, in file order
    pub fn entries(&self) -> &[HsnSacEntry] {
        &self.entries
    }

    /// Find the entry for a code
    ///
    /// Tries the exact code first, then its 6- and 4-digit headings, so an 8-digit tariff item
    /// such as `84713010` resolves to `847130` or `8471`. Malformed codes return `None`.
    pub fn lookup(&self, code: &str) -> Option<&HsnSacEntry> {
        if !is_valid_hsn_sac(code) {
            return None;
        }

        [code.len(), 6, 4]
            .into_iter()
            .filter(|&len| len <= code.len())
            .find_map(|len| self.index.get(&code[..len]))
            .map(|&i| &self.entries[i])
    }

    /// Default GST rate for a code, if the master knows it
    pub fn default_rate(&self, code: &str) -> Option<BigDecimal> {
        self.lookup(code).map(|entry| entry.gst_rate.clone())
    }
}

/// Whether a code has the shape of an HSN/SAC code: 4, 6 or 8 ASCII digits
pub(crate) fn is_valid_hsn_sac(code: &str) -> bool {
    matches!(code.len(), 4 | 6 | 8) && code.bytes().all(|b| b.is_ascii_digit())
}
