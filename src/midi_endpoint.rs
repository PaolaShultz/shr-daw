//! Stable ALSA MIDI endpoint identities and deterministic matching.
//!
//! RtMidi/midir append a volatile numeric ALSA `client:port` address to the
//! meaningful `client-name:port-name` identity.  Persist only the latter.

use anyhow::{bail, Result};

/// Remove one volatile trailing numeric ALSA address and otherwise preserve
/// punctuation, especially the meaningful client/port-name colon.
pub fn stable_identity(name: &str) -> String {
    let trimmed = name.trim();
    let Some((prefix, token)) = trimmed.rsplit_once(char::is_whitespace) else {
        return trimmed.to_owned();
    };
    if numeric_address(token) {
        prefix.trim_end().to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn numeric_address(token: &str) -> bool {
    let Some((client, port)) = token.split_once(':') else {
        return false;
    };
    !client.is_empty()
        && !port.is_empty()
        && client.chars().all(|c| c.is_ascii_digit())
        && port.chars().all(|c| c.is_ascii_digit())
}

/// Compatibility key for old configuration which replaced the meaningful
/// colon with whitespace.  It deliberately performs no partial/fuzzy match.
fn legacy_key(name: &str) -> String {
    stable_identity(name)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve exactly one live endpoint.  Stable canonical identity wins;
/// legacy punctuation/whitespace compatibility is accepted only when unique.
pub fn matching_index(names: &[String], wanted: &str, description: &str) -> Result<usize> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        bail!("{description} identity cannot be empty");
    }
    let stable_wanted = stable_identity(wanted);
    let exact = names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            stable_identity(name)
                .eq_ignore_ascii_case(&stable_wanted)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    match exact.as_slice() {
        [index] => return Ok(*index),
        [_, _, ..] => bail!(
            "{description} {wanted:?} is ambiguous ({} stable identity matches)",
            exact.len()
        ),
        [] => {}
    }

    let compatibility = legacy_key(wanted);
    let legacy = names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| (legacy_key(name) == compatibility).then_some(index))
        .collect::<Vec<_>>();
    match legacy.as_slice() {
        [index] => Ok(*index),
        [] => bail!("{description} {wanted:?} is offline"),
        _ => bail!(
            "{description} {wanted:?} is ambiguous ({} legacy identity matches)",
            legacy.len()
        ),
    }
}

pub fn matching_optional_index(
    names: &[String],
    wanted: &str,
    description: &str,
) -> Result<Option<usize>> {
    match matching_index(names, wanted, description) {
        Ok(index) => Ok(Some(index)),
        Err(error) if error.to_string().ends_with(" is offline") => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_identity_strips_only_a_trailing_numeric_address() {
        assert_eq!(
            stable_identity("AudioBox USB 96:AudioBox USB 96 MIDI 1 32:0"),
            "AudioBox USB 96:AudioBox USB 96 MIDI 1"
        );
        assert_eq!(stable_identity("Device 32:0 name"), "Device 32:0 name");
        assert_eq!(stable_identity("Device:Port"), "Device:Port");
    }

    #[test]
    fn canonical_and_legacy_audiobox_values_resolve_uniquely() {
        let names = vec!["AudioBox USB 96:AudioBox USB 96 MIDI 1 32:0".into()];
        assert_eq!(
            matching_index(
                &names,
                "AudioBox USB 96:AudioBox USB 96 MIDI 1",
                "MIDI output"
            )
            .unwrap(),
            0
        );
        assert_eq!(
            matching_index(
                &names,
                "AudioBox USB 96 AudioBox USB 96 MIDI 1",
                "MIDI output"
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn missing_ambiguous_and_partial_values_are_rejected() {
        let names = vec![
            "Box:Port 20:0".into(),
            "Box-Port 21:0".into(),
            "Other:DIN 22:0".into(),
        ];
        assert!(matching_index(&names, "Box", "MIDI output").is_err());
        assert!(matching_index(&names, "Missing:Port", "MIDI output").is_err());
        assert!(matching_index(&names, "Box Port", "MIDI output")
            .unwrap_err()
            .to_string()
            .contains("ambiguous"));
    }
}
