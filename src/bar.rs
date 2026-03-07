/// Visual progress bar for context window.

use crate::icons::{BAR_EMPTY, BAR_FULL};

pub fn render(percentage: f64, width: usize) -> String {
    let filled = ((percentage / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar: String = std::iter::repeat(BAR_FULL)
        .take(filled)
        .chain(std::iter::repeat(BAR_EMPTY).take(empty))
        .collect();
    format!("[{bar}]")
}
