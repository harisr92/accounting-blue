//! Drawing primitives in millimetres measured from the top-left corner of an A4 page

use super::fonts::Face;
use printpdf::{Color, Greyscale, Line, LinePoint, Mm, Op, PaintMode, Point, Pt, Rect, TextItem};

/// A4 width in millimetres
pub(super) const PAGE_WIDTH: f32 = 210.0;
/// A4 height in millimetres
pub(super) const PAGE_HEIGHT: f32 = 297.0;
/// Thickness of rules and frames, in points
const RULE_WIDTH_PT: f32 = 0.5;
/// Ink for text and rules
const BLACK: f32 = 0.0;

/// How text sits relative to its x position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Align {
    /// Starts at x
    Left,
    /// Centred on x
    Center,
    /// Ends at x
    Right,
}

/// A PDF point for a position measured from the top-left corner
fn point(x: f32, y: f32) -> Point {
    Point::new(Mm(x), Mm(PAGE_HEIGHT - y))
}

/// Ops that reset the pen at the start of a page
pub(super) fn page_setup() -> Vec<Op> {
    vec![
        Op::SetOutlineThickness {
            pt: Pt(RULE_WIDTH_PT),
        },
        Op::SetOutlineColor { col: grey(BLACK) },
        Op::SetFillColor { col: grey(BLACK) },
    ]
}

/// A grey level from 0 (black) to 1 (white)
fn grey(level: f32) -> Color {
    Color::Greyscale(Greyscale::new(level, None))
}

/// `text` in `face` at `size` points, on the baseline `y`, aligned to `x`
pub(super) fn text(face: &Face, size: f32, x: f32, y: f32, align: Align, text: &str) -> Vec<Op> {
    let x = match align {
        Align::Left => x,
        Align::Center => x - face.width_mm(text, size) / 2.0,
        Align::Right => x - face.width_mm(text, size),
    };
    vec![
        Op::StartTextSection,
        Op::SetFont {
            font: face.handle.clone(),
            size: Pt(size),
        },
        Op::SetTextCursor { pos: point(x, y) },
        Op::ShowText {
            items: vec![TextItem::Text(text.to_string())],
        },
        Op::EndTextSection,
    ]
}

/// Like [`text`], in grey
pub(super) fn grey_text(
    face: &Face,
    size: f32,
    level: f32,
    (x, y): (f32, f32),
    align: Align,
    content: &str,
) -> Vec<Op> {
    std::iter::once(Op::SetFillColor { col: grey(level) })
        .chain(text(face, size, x, y, align, content))
        .chain(std::iter::once(Op::SetFillColor { col: grey(BLACK) }))
        .collect()
}

/// A horizontal rule from `x1` to `x2` at `y`
pub(super) fn rule(x1: f32, x2: f32, y: f32) -> Op {
    Op::DrawLine {
        line: Line {
            points: [x1, x2]
                .into_iter()
                .map(|x| LinePoint {
                    p: point(x, y),
                    bezier: false,
                })
                .collect(),
            is_closed: false,
        },
    }
}

/// The outline of a box whose top-left corner is at (`x`, `y`)
pub(super) fn frame(x: f32, y: f32, width: f32, height: f32) -> Op {
    let lower_left = point(x, y + height);
    Op::DrawRectangle {
        rectangle: Rect {
            x: lower_left.x,
            y: lower_left.y,
            width: Mm(width).into(),
            height: Mm(height).into(),
            mode: Some(PaintMode::Stroke),
            winding_order: None,
        },
    }
}
