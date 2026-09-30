//! Fitting text into a width, given a way to measure it

/// Marks text cut short to fit its column
const ELLIPSIS: &str = "...";

/// `text` as it fits in `max_width`, cut short with an ellipsis when it is too wide
pub(super) fn fit(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_width {
        return text.to_string();
    }
    let prefixes = text.char_indices().map(|(end, _)| &text[..end]).rev();
    prefixes
        .map(|prefix| format!("{}{ELLIPSIS}", prefix.trim_end()))
        .find(|candidate| measure(candidate) <= max_width)
        .unwrap_or_default()
}

/// `text` broken into lines at spaces so each fits in `max_width`
///
/// A single word wider than the line gets a line of its own, cut short with [`fit`].
pub(super) fn wrap(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let lines = text
        .split_whitespace()
        .fold(Vec::<String>::new(), |mut lines, word| {
            let joined = lines.last().map(|line| format!("{line} {word}"));
            match joined {
                Some(joined) if measure(&joined) <= max_width => {
                    lines.pop();
                    lines.push(joined);
                }
                _ => lines.push(word.to_string()),
            }
            lines
        });
    lines
        .into_iter()
        .map(|line| fit(&line, max_width, &measure))
        .collect()
}
