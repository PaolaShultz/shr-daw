//! Terminal-cell-aware compact text contracts for the 40-column UI.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Return one line which occupies no more than `cells`, using an ellipsis only
/// for unpredictable dynamic text. Newlines are made visible as spaces.
pub fn fit_line(text: &str, cells: usize) -> String {
    let text = text.replace(['\r', '\n'], " ");
    if width(&text) <= cells {
        return text;
    }
    if cells == 0 {
        return String::new();
    }
    if cells == 1 {
        return "…".into();
    }
    let target = cells - 1;
    let mut used = 0;
    let mut fitted = String::new();
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > target {
            break;
        }
        fitted.push(character);
        used += character_width;
    }
    fitted.push('…');
    fitted
}

pub fn label_value(label: &str, value: &str, cells: usize) -> String {
    if cells == 0 {
        return String::new();
    }
    let value = fit_line(value, cells.saturating_sub(2));
    let value_width = width(&value);
    let label_budget = cells.saturating_sub(value_width + 1);
    let label = fit_line(label, label_budget);
    let gap = cells.saturating_sub(width(&label) + value_width);
    format!("{label}{}{value}", " ".repeat(gap))
}

/// Reserve a fixed left field so a dynamic value can never erase an
/// operational row label or selection marker.
pub fn fixed_label_value(label: &str, label_cells: usize, value: &str, cells: usize) -> String {
    let label_cells = label_cells.min(cells);
    let label = fit_line(label, label_cells);
    if label_cells == cells {
        return label;
    }
    let value_cells = cells - label_cells;
    format!(
        "{label}{}{}",
        " ".repeat(label_cells.saturating_sub(width(&label))),
        fit_line(value, value_cells)
    )
}

/// A concise display-only endpoint label. The canonical identity is retained
/// by callers for matching and persistence.
pub fn endpoint_label(identity: &str, cells: usize) -> String {
    let stable = crate::midi_endpoint::stable_identity(identity);
    let compact = stable.split_once(':').map_or_else(
        || stable.clone(),
        |(client, port)| {
            let port = port
                .strip_prefix(client)
                .map(str::trim_start)
                .filter(|port| !port.is_empty())
                .unwrap_or(port);
            format!("{} · {}", client.trim(), port.trim())
        },
    );
    fit_line(&compact, cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_uses_terminal_cells_and_never_keeps_a_newline() {
        assert_eq!(width(&fit_line("ab·界cd", 6)), 6);
        assert_eq!(fit_line("one\ntwo", 20), "one two");
        assert!(width(&fit_line("wide 界 endpoint", 8)) <= 8);
    }

    #[test]
    fn label_value_preserves_the_right_side_value() {
        let row = label_value("IMPOSSIBLY LONG LABEL", "ONLINE", 18);
        assert_eq!(width(&row), 18);
        assert!(row.ends_with("ONLINE"));
    }

    #[test]
    fn fixed_label_value_preserves_the_operational_label() {
        let row = fixed_label_value(
            ">DEVICE",
            9,
            "IMPOSSIBLY LONG PROFILE NAME · UNVERIFIED",
            38,
        );
        assert_eq!(width(&row), 38);
        assert!(row.starts_with(">DEVICE"));
    }

    #[test]
    fn audiobox_label_removes_only_the_repeated_client_name() {
        assert_eq!(
            endpoint_label("AudioBox USB 96:AudioBox USB 96 MIDI 1 32:0", 38),
            "AudioBox USB 96 · MIDI 1"
        );
    }
}
