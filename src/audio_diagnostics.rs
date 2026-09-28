//! Best-effort, bounded incident log. Called only by the graph's owner thread.
use crate::audio_graph::GraphDefinition;
use crate::effects::{EffectControl, SafetyEvents};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 1_048_576;

struct EffectWatch {
    label: String,
    control: Arc<EffectControl>,
    previous: SafetyEvents,
}

pub(crate) struct AudioDiagnostics {
    path: PathBuf,
    session: u128,
    last_poll: Option<Instant>,
    previous: [u64; 3],
    effects: Vec<EffectWatch>,
    routing: String,
    header_pending: bool,
    error_reported: bool,
}

impl AudioDiagnostics {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            session: unix_millis(),
            last_poll: None,
            previous: [0; 3],
            effects: Vec::new(),
            routing: String::new(),
            header_pending: true,
            error_reported: false,
        }
    }

    pub(crate) fn watch(
        &mut self,
        definition: &GraphDefinition,
        controls: &BTreeMap<u32, Arc<EffectControl>>,
        baseline: bool,
    ) {
        self.effects = controls
            .iter()
            .map(|(&id, control)| {
                let location = if let Some(aux) = definition
                    .aux_buses
                    .iter()
                    .find(|aux| aux.effects.contains(&id))
                {
                    format!("aux={}", aux.id)
                } else if definition.master_chain.contains(&id) {
                    "master".into()
                } else {
                    "insert".into()
                };
                EffectWatch {
                    label: format!("{location} effect={id} kind={:?}", control.kind()),
                    control: Arc::clone(control),
                    previous: if baseline {
                        control.safety_events()
                    } else {
                        SafetyEvents::default()
                    },
                }
            })
            .collect();
        self.routing = definition
            .aux_buses
            .iter()
            .map(|aux| format!("aux={} return_db={:.3}", aux.id, aux.return_gain_db))
            .collect::<Vec<_>>()
            .join(" ");
        self.header_pending = true;
    }

    pub(crate) fn poll(
        &mut self,
        totals: [u64; 3],
        force: bool,
        context: impl FnOnce() -> String,
    ) -> Option<String> {
        if !force
            && self
                .last_poll
                .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return None;
        }
        self.last_poll = Some(Instant::now());
        let snapshots: Vec<_> = self
            .effects
            .iter()
            .map(|watch| watch.control.safety_events())
            .collect();
        let changed = totals != self.previous
            || self
                .effects
                .iter()
                .zip(&snapshots)
                .any(|(watch, now)| now.resets != watch.previous.resets);
        if !changed && !self.header_pending {
            return None;
        }
        let mut entries = Vec::new();
        if self.header_pending {
            entries.push("graph_configuration".to_owned());
        }
        for (index, name) in ["jack_xrun", "shr_callback_deadline", "oversized_callback"]
            .iter()
            .enumerate()
        {
            let count = totals[index].saturating_sub(self.previous[index]);
            if count > 0 {
                entries.push(format!(
                    "event={name} count={count} total={}",
                    totals[index]
                ));
            }
        }
        for (watch, now) in self.effects.iter().zip(&snapshots) {
            let count = now.resets.saturating_sub(watch.previous.resets);
            if count > 0 {
                entries.push(format!(
                    "event=effect_safety_reset {} count={count} non_finite={} lifetime_peak={:.6}",
                    watch.label,
                    now.non_finite.saturating_sub(watch.previous.non_finite),
                    now.peak,
                ));
            }
            // These are observed control targets, not a sample-exact snapshot.
            entries.push(format!(
                "settings {} parameters={:?}",
                watch.label,
                watch.control.diagnostic_parameters()
            ));
        }
        entries.push(format!("routing {}", self.routing));
        entries.push(context());
        let timestamp = unix_millis();
        let text: String = entries
            .iter()
            .map(|entry| {
                format!(
                    "observed_unix_ms={timestamp} pid={} session={} {entry}\n",
                    std::process::id(),
                    self.session
                )
            })
            .collect();
        match append_bounded(&self.path, &text) {
            Ok(()) => {
                self.previous = totals;
                for (watch, now) in self.effects.iter_mut().zip(snapshots) {
                    watch.previous = now;
                }
                self.header_pending = false;
                self.error_reported = false;
                None
            }
            Err(error) => {
                // Retain baselines for retry, but don't flood the status row.
                if self.error_reported {
                    return None;
                }
                self.error_reported = true;
                Some(format!("AUDIO LOG FAILED · {error}"))
            }
        }
    }
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn append_bounded(path: &std::path::Path, text: &str) -> io::Result<()> {
    if text.len() as u64 > MAX_LOG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio diagnostic batch exceeds log bound",
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.len().saturating_add(text.len() as u64) > MAX_LOG_BYTES => {
            std::fs::rename(path, path.with_extension("log.previous"))?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_rotates_and_retains_previous_incidents() {
        let directory = std::env::temp_dir().join(format!(
            "shr-audio-log-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let path = directory.join("audio-diagnostics.log");
        append_bounded(&path, "older\n").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_LOG_BYTES)
            .unwrap();
        append_bounded(&path, "newer\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "newer\n");
        assert_eq!(
            std::fs::metadata(path.with_extension("log.previous"))
                .unwrap()
                .len(),
            MAX_LOG_BYTES
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_log_write_retains_counts_for_retry() {
        let directory = std::env::temp_dir().join(format!(
            "shr-audio-retry-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let path = directory.join("audio-diagnostics.log");
        std::fs::create_dir_all(&path).unwrap();
        let mut log = AudioDiagnostics::new(path.clone());
        assert!(log.poll([2, 1, 0], true, String::new).is_some());
        assert!(log.poll([3, 1, 0], true, String::new).is_none());
        std::fs::remove_dir(&path).unwrap();
        assert!(log.poll([4, 1, 0], true, String::new).is_none());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("event=jack_xrun count=4 total=4"));
        assert!(text.contains("event=shr_callback_deadline count=1 total=1"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn counters_are_grouped_without_repeating_unchanged_events() {
        let directory = std::env::temp_dir().join(format!(
            "shr-audio-counts-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let path = directory.join("audio-diagnostics.log");
        let mut log = AudioDiagnostics::new(path.clone());
        assert!(log.poll([3, 2, 0], true, || "rate=48000".into()).is_none());
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.contains("event=jack_xrun count=3 total=3"));
        assert!(first.contains("event=shr_callback_deadline count=2 total=2"));
        log.poll([3, 2, 0], true, || {
            panic!("unchanged counters should not collect context")
        });
        assert_eq!(first, std::fs::read_to_string(&path).unwrap());
        log.poll([4, 2, 0], true, || "rate=48000".into());
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("event=jack_xrun count=1 total=4"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
