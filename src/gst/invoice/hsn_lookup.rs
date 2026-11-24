//! HSN/SAC master data and default GST rates
//!
//! The master is compiled into the crate from two files:
//!
//! - `src/gst/data/hsn_master.json`: the codes whose default rate has been checked against the GST
//!   schedule named by [`HsnMaster::schedule`], with their rates. The notifications and references
//!   it was checked against are listed under `sources` there. Always compiled in with `india`.
//! - `src/gst/data/hsn_codes.json`: every 4-, 6- and 8-digit HSN code and 6-digit SAC code with
//!   its description, from the GST portal's HSN/SAC master (built by
//!   `scripts/build_hsn_codes.py`, which names the source and the day it was downloaded). It adds
//!   about 1.8 MB to the binary, so it is only compiled in with the `hsn-catalog` feature.
//!
//! Without `hsn-catalog` the embedded master holds only the checked codes, so
//! [`HsnMaster::lists_every_code`] is `false` and validation does not warn about codes it lacks.
//!
//! The portal file has no rates, so most codes have none: [`HsnMaster::default_rate`] falls back
//! to the nearest heading that has one. Rates are defaults: some goods and services carry
//! conditional rates (price thresholds, input tax credit options), so callers can always pass an
//! explicit rate instead.

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

/// The embedded master, parsed once on first use
///
/// The JSON is compiled into the crate, so a parse failure is a build defect rather than a
/// runtime condition; `test_embedded_master_parses` catches it in CI.
static MASTER: LazyLock<HsnMaster> =
    LazyLock::new(|| HsnMaster::from_parts(embedded_rates(), embedded_codes()));

/// The embedded checked rates
#[allow(clippy::expect_used)]
fn embedded_rates() -> RawRates {
    serde_json::from_str(include_str!("../data/hsn_master.json"))
        .expect("embedded HSN/SAC rates are valid JSON")
}

/// The embedded portal code list
#[cfg(feature = "hsn-catalog")]
#[allow(clippy::expect_used)]
fn embedded_codes() -> Option<RawCodes> {
    Some(
        serde_json::from_str(include_str!("../data/hsn_codes.json"))
            .expect("embedded HSN/SAC codes are valid JSON"),
    )
}

/// No portal code list: the `hsn-catalog` feature is off
#[cfg(not(feature = "hsn-catalog"))]
fn embedded_codes() -> Option<RawCodes> {
    None
}

/// Whether a code classifies goods or services
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HsnSacKind {
    /// Harmonized System of Nomenclature code (goods)
    Hsn,
    /// Services Accounting Code (services)
    Sac,
}

/// Chapter of the classification that holds every SAC code; all other chapters are goods
const SAC_CHAPTER: &str = "99";

impl HsnSacKind {
    /// Whether a code classifies goods or services, from its chapter: SAC codes start with `99`
    ///
    /// The code's shape is not checked; see [`HsnMaster::lookup`] for well-formed codes.
    #[must_use]
    pub fn of_code(code: &str) -> Self {
        if code.starts_with(SAC_CHAPTER) {
            Self::Sac
        } else {
            Self::Hsn
        }
    }
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
    /// Default total GST rate as a percentage (e.g. 18 for 18%), when it has been checked for
    /// this code; `None` for most codes, which the portal's list gives no rate
    pub gst_rate: Option<BigDecimal>,
}

/// Lookup table of HSN/SAC codes and their default GST rates
///
/// Deserializes from the shape of `hsn_master.json` (`schedule`, `effective_from` and `entries`
/// with rates), so a caller can check documents against a master of its own.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawRates")]
pub struct HsnMaster {
    schedule: String,
    effective_from: NaiveDate,
    /// Ordered by code
    entries: Vec<HsnSacEntry>,
    index: HashMap<String, usize>,
    /// Each entry's description in lower case, in `entries` order, for [`HsnMaster::search`]
    search_text: Vec<String>,
    /// Whether every valid code is listed, so a code missing from it is unknown
    lists_every_code: bool,
}

/// On-disk shape of the checked rates
#[derive(Deserialize)]
struct RawRates {
    schedule: String,
    effective_from: NaiveDate,
    entries: Vec<RawRate>,
}

/// One checked code: its description and default rate
#[derive(Deserialize)]
struct RawRate {
    code: String,
    description: String,
    gst_rate: BigDecimal,
}

/// On-disk shape of the portal's code list: `[code, description]` pairs
#[derive(Deserialize)]
struct RawCodes {
    codes: Vec<(String, String)>,
}

/// A caller's own master is taken as the whole list of codes it accepts
impl From<RawRates> for HsnMaster {
    fn from(rates: RawRates) -> Self {
        let mut master = Self::from_parts(rates, None);
        master.lists_every_code = true;
        master
    }
}

/// Headings a code falls back to, longest first: the code itself, then its 6- and 4-digit headings
const HEADING_LENGTHS: [usize; 2] = [6, 4];

impl HsnMaster {
    /// The master over `codes`, with the checked `rates` (and their wording) laid over them. A
    /// checked code missing from the list is added. Without `codes` it holds only the checked
    /// codes and does not list every code.
    fn from_parts(rates: RawRates, codes: Option<RawCodes>) -> Self {
        let lists_every_code = codes.is_some();
        let checked: HashMap<String, RawRate> = rates
            .entries
            .into_iter()
            .map(|rate| (rate.code.clone(), rate))
            .collect();
        let listed: HashMap<String, String> = codes
            .map(|codes| codes.codes.into_iter().collect())
            .unwrap_or_default();
        let mut all: Vec<&String> = listed.keys().chain(checked.keys()).collect();
        all.sort();
        all.dedup();
        let entries: Vec<HsnSacEntry> = all
            .into_iter()
            .map(|code| match checked.get(code) {
                Some(rate) => entry(code, &rate.description, Some(rate.gst_rate.clone())),
                None => entry(code, listed.get(code).map_or("", String::as_str), None),
            })
            .collect();
        Self::indexed(
            rates.schedule,
            rates.effective_from,
            entries,
            lists_every_code,
        )
    }

    fn indexed(
        schedule: String,
        effective_from: NaiveDate,
        entries: Vec<HsnSacEntry>,
        lists_every_code: bool,
    ) -> Self {
        let index = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.code.clone(), i))
            .collect();
        let search_text = entries
            .iter()
            .map(|e| e.description.to_lowercase())
            .collect();
        Self {
            schedule,
            effective_from,
            entries,
            index,
            search_text,
            lists_every_code,
        }
    }

    /// The master embedded in the crate
    #[must_use]
    pub fn global() -> &'static HsnMaster {
        &MASTER
    }

    /// Name of the GST rate schedule the default rates follow
    #[must_use]
    pub fn schedule(&self) -> &str {
        &self.schedule
    }

    /// Date the rate schedule took effect
    #[must_use]
    pub fn effective_from(&self) -> NaiveDate {
        self.effective_from
    }

    /// Whether the master lists every valid code, so that a code it lacks is unknown
    ///
    /// `true` for a master deserialized by the caller, and for [`HsnMaster::global`] with the
    /// `hsn-catalog` feature. Without that feature the embedded master holds only the codes with
    /// checked rates, and this is `false`.
    #[must_use]
    pub fn lists_every_code(&self) -> bool {
        self.lists_every_code
    }

    /// All entries, ordered by code
    #[must_use]
    pub fn entries(&self) -> &[HsnSacEntry] {
        &self.entries
    }

    /// The code and the headings it falls back to that the master holds, longest first
    fn candidates(&self, code: &str) -> Vec<&HsnSacEntry> {
        std::iter::once(code.len())
            .chain(HEADING_LENGTHS.into_iter().filter(|&len| len < code.len()))
            .filter_map(|len| self.index.get(&code[..len]))
            .map(|&i| &self.entries[i])
            .collect()
    }

    /// Find the entry for a code
    ///
    /// Tries the exact code first, then its 6- and 4-digit headings, so an 8-digit tariff item
    /// such as `84713010` resolves to `847130` or `8471` when the master lacks it. A code is known
    /// when it or one of its headings is. Malformed codes return `None`.
    #[must_use]
    pub fn lookup(&self, code: &str) -> Option<&HsnSacEntry> {
        if !is_valid_hsn_sac(code) {
            return None;
        }
        self.candidates(code).into_iter().next()
    }

    /// Default GST rate for a code: that of the code, or of the nearest heading over it that has
    /// one. `None` when neither the code nor its headings has a checked rate.
    #[must_use]
    pub fn default_rate(&self, code: &str) -> Option<BigDecimal> {
        if !is_valid_hsn_sac(code) {
            return None;
        }
        self.candidates(code)
            .into_iter()
            .find_map(|entry| entry.gst_rate.clone())
    }

    /// Up to `limit` entries matching `query`, for picking a code
    ///
    /// Digits match codes starting with them; anything else matches descriptions containing every
    /// word of it, ignoring case. Headings come before the tariff items under them (shorter codes
    /// first), then codes in order. A blank query matches nothing.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<&HsnSacEntry> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let words: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        let by_code = query.bytes().all(|b| b.is_ascii_digit());
        let mut found: Vec<&HsnSacEntry> = self
            .entries
            .iter()
            .zip(&self.search_text)
            .filter(|(entry, text)| {
                if by_code {
                    entry.code.starts_with(query)
                } else {
                    words.iter().all(|word| text.contains(word.as_str()))
                }
            })
            .map(|(entry, _)| entry)
            .collect();
        // Stable: entries are already in code order.
        found.sort_by_key(|entry| entry.code.len());
        found.truncate(limit);
        found
    }
}

/// An entry for `code`, its kind read from its chapter
fn entry(code: &str, description: &str, gst_rate: Option<BigDecimal>) -> HsnSacEntry {
    HsnSacEntry {
        code: code.to_string(),
        kind: HsnSacKind::of_code(code),
        description: description.to_string(),
        gst_rate,
    }
}

/// Whether a code has the shape of an HSN/SAC code: 4, 6 or 8 ASCII digits
pub(crate) fn is_valid_hsn_sac(code: &str) -> bool {
    matches!(code.len(), 4 | 6 | 8) && code.bytes().all(|b| b.is_ascii_digit())
}
