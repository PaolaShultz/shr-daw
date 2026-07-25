//! Terminal-cell-aware compact text contracts for the 40-column UI.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn sanitize_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Return one line which occupies no more than `cells`, using an ellipsis only
/// for unpredictable dynamic text. Display controls are made visible as spaces.
pub fn fit_line(text: &str, cells: usize) -> String {
    let text = sanitize_line(text);
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

/// Fit an identity while retaining both its beginning and distinguishing tail.
pub fn fit_middle(text: &str, cells: usize) -> String {
    let text = sanitize_line(text);
    if width(&text) <= cells {
        return text;
    }
    if cells == 0 {
        return String::new();
    }
    if cells == 1 {
        return "…".into();
    }

    let content_cells = cells - 1;
    let prefix_cells = content_cells.div_ceil(2);
    let suffix_cells = content_cells - prefix_cells;
    let mut prefix = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > prefix_cells {
            break;
        }
        prefix.push(character);
        used += character_width;
    }

    let mut suffix = Vec::new();
    used = 0;
    for character in text.chars().rev() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > suffix_cells {
            break;
        }
        suffix.push(character);
        used += character_width;
    }
    suffix.reverse();
    format!("{prefix}…{}", suffix.into_iter().collect::<String>())
}

/// Keep an operational suffix visible after fitting unpredictable text.
pub fn fit_with_suffix(text: &str, suffix: &str, cells: usize) -> String {
    let suffix = sanitize_line(suffix);
    let suffix_width = width(&suffix);
    if suffix_width >= cells {
        return fit_middle(&suffix, cells);
    }
    let text_cells = cells - suffix_width;
    format!("{}{}", fit_line(text, text_cells), suffix)
}

/// Fit a status while retaining its final consequence or recovery clause.
pub fn fit_status(text: &str, cells: usize) -> String {
    let text = sanitize_line(text);
    if width(&text) <= cells {
        return text;
    }
    if let Some((head, tail)) = text.rsplit_once(" · ") {
        return fit_with_suffix(head, &format!(" · {tail}"), cells);
    }
    fit_line(&text, cells)
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
        assert_eq!(fit_line("one\ttwo", 20), "one two");
        assert!(width(&fit_line("wide 界 endpoint", 8)) <= 8);
    }

    #[test]
    fn middle_fit_preserves_both_ends_in_terminal_cells() {
        let fitted = fit_middle("Project 夜明けのシンセ.wav", 15);
        assert!(fitted.starts_with("Project"));
        assert!(fitted.ends_with(".wav"));
        assert!(width(&fitted) <= 15);
    }

    #[test]
    fn suffix_fit_never_loses_the_recovery() {
        let fitted = fit_with_suffix("IMPOSSIBLY LONG ROUTING FAILURE DETAIL", " · retry", 20);
        assert!(fitted.ends_with(" · retry"));
        assert!(width(&fitted) <= 20);
    }

    #[test]
    fn status_fit_keeps_the_last_recovery_clause() {
        let fitted = fit_status("PERFORMANCE · FINAL BUS UNAVAILABLE · retry MIX", 38);
        assert!(fitted.ends_with(" · retry MIX"));
        assert!(width(&fitted) <= 38);
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
