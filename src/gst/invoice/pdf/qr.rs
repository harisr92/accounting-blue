//! QR codes drawn as vector squares, for the UPI payment link

use super::draw::filled_squares;
use super::PdfError;
use printpdf::Op;
use qrcode::{Color, EcLevel, QrCode};

/// The modules of a QR code, row by row from the top; `true` is dark
pub(super) type QrModules = Vec<Vec<bool>>;

/// Encode `data` as a QR code at error correction level M (about 15% of it can be lost)
///
/// # Errors
///
/// [`PdfError::QrCode`] if `data` is too long for a QR code.
pub(super) fn qr_modules(data: &str) -> Result<QrModules, PdfError> {
    let code = QrCode::with_error_correction_level(data, EcLevel::M).map_err(|error| {
        PdfError::QrCode {
            reason: error.to_string(),
        }
    })?;
    let width = code.width().max(1);
    Ok(code
        .to_colors()
        .chunks(width)
        .map(|row| row.iter().map(|color| *color == Color::Dark).collect())
        .collect())
}

/// Ops drawing `modules` as a `size` millimetre square whose top-left corner is at (`x`, `y`)
pub(super) fn qr_ops(modules: &[Vec<bool>], (x, y): (f32, f32), size: f32) -> Vec<Op> {
    if modules.is_empty() {
        return Vec::new();
    }
    let side = size / modules.len() as f32;
    let corners = modules.iter().enumerate().flat_map(|(row, cells)| {
        cells
            .iter()
            .enumerate()
            .filter(|(_, dark)| **dark)
            .map(move |(column, _)| (x + side * column as f32, y + side * row as f32))
    });
    vec![filled_squares(corners, side)]
}
