//! Font faces for the PDF renderer, with the metrics to measure text

use super::{FontFace, PdfError, PdfFont};
use printpdf::{BuiltinFont, ParsedFont, PdfDocument, PdfFontHandle};

/// Points per millimetre
const PT_PER_MM: f32 = 72.0 / 25.4;
/// Width, in ems, of a character with no glyph in the font. Rendering refuses such characters
/// unless they are whitespace, and printpdf's built-in Helvetica has no space glyph, so this is
/// the width of the standard Helvetica space.
const MISSING_GLYPH_EM: f32 = 0.278;

/// One face: the handle the page ops use and the metrics to measure text with
#[derive(Debug, Clone)]
pub(super) struct Face {
    pub(super) handle: PdfFontHandle,
    metrics: ParsedFont,
}

impl Face {
    /// Whether the face can draw `c`
    pub(super) fn has_glyph(&self, c: char) -> bool {
        self.metrics.lookup_glyph_index(u32::from(c)).is_some()
    }

    /// Width of `text` set at `size` points, in millimetres
    pub(super) fn width_mm(&self, text: &str, size: f32) -> f32 {
        let units_per_em = f32::from(self.metrics.units_per_em.max(1));
        let ems: f32 = text
            .chars()
            .map(|c| {
                self.metrics
                    .lookup_glyph_index(u32::from(c))
                    .and_then(|glyph| self.metrics.get_glyph_width(glyph))
                    .map_or(MISSING_GLYPH_EM, |width| f32::from(width) / units_per_em)
            })
            .sum();
        ems * size / PT_PER_MM
    }
}

/// The regular and bold faces an invoice is set in
#[derive(Debug, Clone)]
pub(super) struct Fonts {
    pub(super) regular: Face,
    pub(super) bold: Face,
}

impl Fonts {
    /// The face a page op refers to by `handle`, if it is one of these
    pub(super) fn face(&self, handle: &PdfFontHandle) -> Option<&Face> {
        [&self.regular, &self.bold]
            .into_iter()
            .find(|face| &face.handle == handle)
    }

    /// Load the faces `font` asks for, registering custom ones with `document`
    pub(super) fn load(document: &mut PdfDocument, font: &PdfFont) -> Result<Self, PdfError> {
        match font {
            PdfFont::Builtin => Ok(Self {
                regular: builtin(BuiltinFont::Helvetica)?,
                bold: builtin(BuiltinFont::HelveticaBold)?,
            }),
            PdfFont::Custom { regular, bold } => {
                let regular_face = custom(document, regular, FontFace::Regular)?;
                let bold_face = match bold {
                    Some(bytes) => custom(document, bytes, FontFace::Bold)?,
                    None => regular_face.clone(),
                };
                Ok(Self {
                    regular: regular_face,
                    bold: bold_face,
                })
            }
        }
    }
}

/// A standard PDF font, measured with the metrics printpdf ships for it
fn builtin(font: BuiltinFont) -> Result<Face, PdfError> {
    let metrics = font.get_parsed_font().ok_or(PdfError::BuiltinFontMetrics)?;
    Ok(Face {
        handle: PdfFontHandle::Builtin(font),
        metrics,
    })
}

/// A TrueType or OpenType font from `bytes`, embedded in `document`
fn custom(document: &mut PdfDocument, bytes: &[u8], face: FontFace) -> Result<Face, PdfError> {
    let metrics =
        ParsedFont::from_bytes(bytes, 0, &mut Vec::new()).ok_or(PdfError::InvalidFont { face })?;
    let id = document.add_font(&metrics);
    Ok(Face {
        handle: PdfFontHandle::External(id),
        metrics,
    })
}
