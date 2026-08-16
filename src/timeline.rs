//! Canonical bounded tick-domain interpretation shared by runtime scheduling,
//! automation chase, metronome beat boundaries, and Standard MIDI File export.

use crate::config::ExternalMidiConfig;
use crate::sequencer::{
    AutomationCurve, AutomationTarget, PageTarget, ScheduledMessage, Song, AUTOMATION_TICKS_PER_ROW,
};
use crate::tempo::Bpm;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::time::Duration;

pub const MAX_TIMELINE_EVENTS: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TempoChange {
    pub tick: u64,
    pub tempo: Bpm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeterChange {
    pub tick: u64,
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickMidiEvent {
    pub tick: u64,
    pub bytes: Vec<u8>,
    pub order: usize,
    pub row: usize,
    pub lane: Option<usize>,
    pub target: Option<PageTarget>,
    pub ordinal: u32,
    pub automation: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomationSegment {
    pub pattern: u16,
    pub order: usize,
    pub lane_id: u32,
    pub target: AutomationTarget,
    pub curve: AutomationCurve,
    pub start_tick: u64,
    pub end_tick: u64,
    pub start_value: u16,
    pub end_value: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickEffectEvent {
    pub tick: u64,
    pub order: usize,
    pub row: usize,
    pub message: crate::sequencer::ScheduledEffectAutomation,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimelinePlan {
    pub ppqn: u16,
    pub start_order: usize,
    pub start_row: usize,
    pub end_tick: u64,
    pub tempos: Vec<TempoChange>,
    pub meters: Vec<MeterChange>,
    pub midi: Vec<TickMidiEvent>,
    pub automation: Vec<AutomationSegment>,
    pub effects: Vec<TickEffectEvent>,
}

impl TimelinePlan {
    pub fn duration_at(&self, tick: u64) -> Duration {
        let mut seconds = 0.0;
        let mut previous_tick = 0u64;
        let mut tempo = self
            .tempos
            .first()
            .map_or(Bpm::DEFAULT, |change| change.tempo);
        for change in self.tempos.iter().skip(1) {
            if change.tick > tick {
                break;
            }
            seconds += ticks_to_seconds(change.tick - previous_tick, self.ppqn, tempo);
            previous_tick = change.tick;
            tempo = change.tempo;
        }
        seconds += ticks_to_seconds(tick.saturating_sub(previous_tick), self.ppqn, tempo);
        Duration::from_secs_f64(seconds.max(0.0))
    }

    pub fn scheduled_messages(&self) -> Vec<ScheduledMessage> {
        let mut messages = self
            .midi
            .iter()
            .map(|event| ScheduledMessage {
                at: self.duration_at(event.tick),
                bytes: event.bytes.clone(),
                order: event.order,
                row: event.row,
                lane: event.lane,
                target: event.target.clone(),
                automation: event.automation,
                effect: None,
            })
            .collect::<Vec<_>>();
        messages.extend(self.effects.iter().map(|event| ScheduledMessage {
            at: self.duration_at(event.tick),
            bytes: Vec::new(),
            order: event.order,
            row: event.row,
            lane: None,
            target: None,
            automation: true,
            effect: Some(event.message.clone()),
        }));
        messages.sort_by_key(|message| {
            (
                message.at,
                if message.effect.is_some() {
                    1
                } else if message.bytes.is_empty() {
                    0
                } else {
                    2
                },
            )
        });
        messages
    }
}

fn ticks_to_seconds(ticks: u64, ppqn: u16, tempo: Bpm) -> f64 {
    ticks as f64 * 60.0 / (tempo.as_f64() * f64::from(ppqn))
}

pub fn compile(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
) -> Result<TimelinePlan> {
    song.validate()?;
    let ppqn_u32 = AUTOMATION_TICKS_PER_ROW
        .checked_mul(u32::from(song.steps_per_beat))
        .context("timeline PPQN overflow")?;
    let ppqn = u16::try_from(ppqn_u32).context("timeline PPQN exceeds Standard MIDI File")?;
    let elapsed = crate::sequencer::schedule_elapsed(song, config, start_order, start_row)?;
    if elapsed.len() > MAX_TIMELINE_EVENTS {
        bail!("timeline exceeds {MAX_TIMELINE_EVENTS} events");
    }

    let (tempos, meters, automation) = musical_maps(song, start_order, start_row)?;
    let end_tick = arrangement_ticks(song, start_order, start_row)?;
    let mut midi = Vec::with_capacity(elapsed.len());
    for (ordinal, message) in elapsed.into_iter().enumerate() {
        let event_tick = tick_at_duration(message.at, ppqn, &tempos, end_tick);
        midi.push(TickMidiEvent {
            tick: event_tick.min(end_tick),
            bytes: message.bytes,
            order: message.order,
            row: message.row,
            lane: message.lane,
            target: message.target,
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            automation: false,
        });
    }

    append_runtime_automation(song, &automation, &mut midi)?;
    let effects = effect_events(&automation);
    if midi.len().saturating_add(effects.len()) > MAX_TIMELINE_EVENTS {
        bail!("timeline exceeds {MAX_TIMELINE_EVENTS} events");
    }
    midi.sort_by_key(|event| (event.tick, event_priority(&event.bytes), event.ordinal));
    suppress_unchanged_automation_cc(&mut midi);
    Ok(TimelinePlan {
        ppqn,
        start_order,
        start_row,
        end_tick,
        tempos,
        meters,
        midi,
        automation,
        effects,
    })
}

fn suppress_unchanged_automation_cc(events: &mut Vec<TickMidiEvent>) {
    let mut values = BTreeMap::<(PageTarget, u8, u8), u8>::new();
    events.retain(|event| {
        let [status, controller, value, ..] = event.bytes.as_slice() else {
            return true;
        };
        if status & 0xf0 != 0xb0 {
            return true;
        }
        let Some(target) = event.target.clone() else {
            return true;
        };
        let key = (target, status & 0x0f, *controller);
        let unchanged = values.get(&key) == Some(value);
        values.insert(key, *value);
        !event.automation || !unchanged
    });
}

fn arrangement_ticks(song: &Song, start_order: usize, start_row: usize) -> Result<u64> {
    song.order
        .iter()
        .enumerate()
        .skip(start_order)
        .try_fold(0u64, |total, (order, number)| {
            let rows = song
                .patterns
                .get(number)
                .context("timeline Pattern is missing")?
                .rows
                .len()
                .saturating_sub(if order == start_order { start_row } else { 0 });
            total
                .checked_add(
                    u64::try_from(rows).unwrap_or_default() * u64::from(AUTOMATION_TICKS_PER_ROW),
                )
                .context("timeline length overflow")
        })
}

fn tick_at_duration(elapsed: Duration, ppqn: u16, tempos: &[TempoChange], end_tick: u64) -> u64 {
    let duration_at = |tick| {
        let mut seconds = 0.0;
        let mut previous_tick = 0;
        let mut tempo = tempos.first().map_or(Bpm::DEFAULT, |change| change.tempo);
        for change in tempos.iter().skip(1) {
            if change.tick > tick {
                break;
            }
            seconds += ticks_to_seconds(change.tick - previous_tick, ppqn, tempo);
            previous_tick = change.tick;
            tempo = change.tempo;
        }
        seconds + ticks_to_seconds(tick.saturating_sub(previous_tick), ppqn, tempo)
    };
    let wanted = elapsed.as_secs_f64();
    let mut low = 0u64;
    let mut high = end_tick;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if duration_at(middle) <= wanted + 1.0e-9 {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

fn effect_events(segments: &[AutomationSegment]) -> Vec<TickEffectEvent> {
    let mut events = Vec::new();
    let mut ordinal = 0u32;
    for segment in segments {
        let (effect_id, effect_kind, effect_version, parameter) = match &segment.target {
            AutomationTarget::Effect {
                effect_id,
                effect_kind,
                effect_version,
                parameter,
                ..
            } => (
                *effect_id,
                *effect_kind,
                *effect_version,
                Some(parameter.clone()),
            ),
            AutomationTarget::EffectBypass {
                effect_id,
                effect_kind,
                effect_version,
                ..
            } => (*effect_id, *effect_kind, *effect_version, None),
            _ => continue,
        };
        let samples =
            if segment.curve == AutomationCurve::Step || segment.start_tick >= segment.end_tick {
                1
            } else {
                64
            };
        for offset in 0..=samples {
            if samples == 1 && offset == 1 {
                continue;
            }
            let tick =
                segment.start_tick + (segment.end_tick - segment.start_tick) * offset / samples;
            let value = if samples == 1 {
                segment.start_value
            } else {
                let delta = i64::from(segment.end_value) - i64::from(segment.start_value);
                (i64::from(segment.start_value) + delta * offset as i64 / samples as i64)
                    .clamp(0, i64::from(u16::MAX)) as u16
            };
            events.push(TickEffectEvent {
                tick,
                order: segment.order,
                row: 0,
                message: crate::sequencer::ScheduledEffectAutomation {
                    effect_id,
                    effect_kind,
                    effect_version,
                    parameter: parameter.clone(),
                    value,
                },
                ordinal,
            });
            ordinal = ordinal.wrapping_add(1);
        }
    }
    events.sort_by_key(|event| (event.tick, event.ordinal));
    events
}

fn musical_maps(
    song: &Song,
    start_order: usize,
    start_row: usize,
) -> Result<(Vec<TempoChange>, Vec<MeterChange>, Vec<AutomationSegment>)> {
    let mut tempos = Vec::new();
    let mut meters = Vec::new();
    let mut automation = Vec::new();
    let mut origin = 0u64;
    for (order, number) in song.order.iter().copied().enumerate().skip(start_order) {
        let pattern = song
            .patterns
            .get(&number)
            .context("timeline Pattern is missing")?;
        let first_row = if order == start_order { start_row } else { 0 };
        push_change(
            &mut tempos,
            TempoChange {
                tick: origin,
                tempo: pattern.tempo,
            },
        );
        if meters
            .last()
            .is_none_or(|meter: &MeterChange| meter.numerator != pattern.meter)
        {
            meters.push(MeterChange {
                tick: origin,
                numerator: pattern.meter,
                denominator: 4,
            });
        }
        for (row, cells) in pattern.rows.iter().enumerate().skip(first_row) {
            let row_tick = origin
                + u64::try_from(row - first_row).unwrap_or_default()
                    * u64::from(AUTOMATION_TICKS_PER_ROW);
            if let Some(tempo) = cells
                .iter()
                .filter_map(|cell| match cell.command {
                    crate::sequencer::Command::Tempo(tempo) => Some(tempo),
                    _ => None,
                })
                .next_back()
            {
                push_change(
                    &mut tempos,
                    TempoChange {
                        tick: row_tick + u64::from(AUTOMATION_TICKS_PER_ROW),
                        tempo,
                    },
                );
            }
        }
        let pattern_ticks = u32::try_from(pattern.rows.len())
            .unwrap_or_default()
            .saturating_mul(AUTOMATION_TICKS_PER_ROW);
        let local_start = u32::try_from(first_row).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW;
        for lane in &pattern.automation {
            append_lane_segments(
                &mut automation,
                lane,
                number,
                order,
                origin,
                local_start,
                pattern_ticks,
            );
        }
        origin = origin
            .checked_add(
                u64::try_from(pattern.rows.len() - first_row).unwrap_or_default()
                    * u64::from(AUTOMATION_TICKS_PER_ROW),
            )
            .context("timeline length overflow")?;
    }
    tempos.retain(|change| change.tick <= origin);
    Ok((tempos, meters, automation))
}

fn push_change(changes: &mut Vec<TempoChange>, change: TempoChange) {
    if let Some(last) = changes.last_mut().filter(|last| last.tick == change.tick) {
        *last = change;
    } else if changes.last().is_none_or(|last| last.tempo != change.tempo) {
        changes.push(change);
    }
}

fn append_lane_segments(
    out: &mut Vec<AutomationSegment>,
    lane: &crate::sequencer::AutomationLane,
    pattern: u16,
    order: usize,
    origin: u64,
    local_start: u32,
    pattern_ticks: u32,
) {
    if lane.points.is_empty() || pattern_ticks == 0 {
        return;
    }
    let mut start_local = local_start;
    while start_local < pattern_ticks {
        let next = lane
            .points
            .iter()
            .find(|point| point.tick > start_local)
            .map_or(pattern_ticks, |point| point.tick);
        let end_local = next.min(pattern_ticks);
        let start_value = lane
            .value_at(start_local, pattern_ticks)
            .unwrap_or_default();
        let end_value = lane
            .value_at(end_local, pattern_ticks)
            .unwrap_or(start_value);
        out.push(AutomationSegment {
            pattern,
            order,
            lane_id: lane.id,
            target: lane.target.clone(),
            curve: lane.curve,
            start_tick: origin + u64::from(start_local - local_start),
            end_tick: origin + u64::from(end_local - local_start),
            start_value,
            end_value,
        });
        if end_local <= start_local {
            break;
        }
        start_local = end_local;
    }
}

fn append_runtime_automation(
    song: &Song,
    segments: &[AutomationSegment],
    midi: &mut Vec<TickMidiEvent>,
) -> Result<()> {
    let mut ordinal = u32::try_from(midi.len()).unwrap_or(u32::MAX / 2);
    for segment in segments {
        let pattern = song
            .patterns
            .get(&segment.pattern)
            .context("automation Pattern is missing")?;
        let (page, channel, controller) = match &segment.target {
            AutomationTarget::MidiCc {
                page,
                channel,
                controller,
            } => (usize::from(*page), *channel, *controller),
            AutomationTarget::Instrument {
                page,
                engine,
                control,
            } => {
                let page = usize::from(*page);
                let channel = pattern
                    .pages
                    .get(page)
                    .and_then(|page| page.columns.first())
                    .context("instrument automation page has no lanes")?
                    .channel;
                let controller = mapped_control_cc(engine, control)
                    .context("instrument automation control is not mapped")?;
                (page, channel, controller)
            }
            AutomationTarget::Effect { .. } | AutomationTarget::EffectBypass { .. } => continue,
        };
        let target = pattern
            .pages
            .get(page)
            .context("automation page is missing")?
            .target
            .clone();
        for (tick, value) in sampled_7bit(segment) {
            midi.push(TickMidiEvent {
                tick,
                bytes: vec![0xb0 | channel, controller, value],
                order: segment.order,
                row: usize::try_from(
                    (tick - segment.start_tick) / u64::from(AUTOMATION_TICKS_PER_ROW),
                )
                .unwrap_or_default(),
                lane: None,
                target: Some(target.clone()),
                ordinal,
                automation: true,
            });
            ordinal = ordinal.wrapping_add(1);
        }
    }
    Ok(())
}

pub fn mapped_control_cc(engine: &str, control: &str) -> Option<u8> {
    let engine: crate::preset::BackendKind = engine.parse().ok()?;
    match engine {
        crate::preset::BackendKind::Synthv1 => crate::control::CONTROLS
            .iter()
            .find(|candidate| candidate.xml_name == control)
            .map(|candidate| candidate.cc),
        crate::preset::BackendKind::MojSint => crate::control::MOJ_MODEL_D_CONTROLS
            .iter()
            .chain(crate::control::MOJ_SIX_OP_PM_CONTROLS.iter())
            .chain(crate::control::MOJ_STRANGE_CONTROLS.iter())
            .chain(crate::control::MOJ_SWARM_CONTROLS.iter())
            .chain(crate::control::MOJ_BASS_MATRIX_CONTROLS.iter())
            .find(|candidate| candidate.macro_id == control)
            .map(|candidate| candidate.cc),
        crate::preset::BackendKind::Yoshimi
        | crate::preset::BackendKind::FluidSynth
        | crate::preset::BackendKind::ShrSampler => {
            (control == "instrument_volume").then_some(crate::control::INSTRUMENT_VOLUME_CC)
        }
    }
}

pub fn sampled_7bit(segment: &AutomationSegment) -> Vec<(u64, u8)> {
    let to_cc = |value: u16| ((u32::from(value) * 127 + 32_767) / 65_535) as u8;
    let first = to_cc(segment.start_value);
    let last = to_cc(segment.end_value);
    if segment.curve == AutomationCurve::Step
        || segment.start_tick >= segment.end_tick
        || first == last
    {
        return vec![(segment.start_tick, first)];
    }
    let distance = u16::from(first.abs_diff(last));
    (0..=distance)
        .map(|offset| {
            let value = if last >= first {
                first + offset as u8
            } else {
                first - offset as u8
            };
            let tick = segment.start_tick
                + (segment.end_tick - segment.start_tick) * u64::from(offset)
                    / u64::from(distance.max(1));
            (tick, value)
        })
        .collect()
}

pub fn event_priority(bytes: &[u8]) -> u8 {
    match bytes {
        [] => 0,
        [status, 0, ..] if status & 0xf0 == 0xb0 => 1,
        [status, 32, ..] if status & 0xf0 == 0xb0 => 2,
        [status, ..] if status & 0xf0 == 0xc0 => 3,
        [status, ..] if status & 0xf0 == 0xb0 => 4,
        [status, ..] if status & 0xf0 == 0x80 => 5,
        [status, _, 0, ..] if status & 0xf0 == 0x90 => 5,
        [status, ..] if status & 0xf0 == 0x90 => 6,
        _ => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{AutomationLane, AutomationPoint};

    #[test]
    fn interpolation_is_exact_and_step_holds() {
        let mut lane = AutomationLane {
            id: 1,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 0,
                controller: 74,
            },
            curve: AutomationCurve::Linear,
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: 120 << 8,
                },
                AutomationPoint {
                    tick: 1_680,
                    value: 112 << 8,
                },
            ],
        };
        assert_eq!(lane.value_at(840, 3_360), Some(116 << 8));
        lane.curve = AutomationCurve::Step;
        assert_eq!(lane.value_at(1_679, 3_360), Some(120 << 8));
        assert_eq!(lane.value_at(1_680, 3_360), Some(112 << 8));
    }

    #[test]
    fn tick_resolution_fits_smf_for_every_supported_grid() {
        for steps in 1..=16 {
            assert!(AUTOMATION_TICKS_PER_ROW * steps <= i16::MAX as u32);
        }
    }

    #[test]
    fn play_here_chases_active_ramp_and_loop_boundary_without_default_gap() {
        let lane = AutomationLane {
            id: 7,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 0,
                controller: 1,
            },
            curve: AutomationCurve::Linear,
            points: vec![
                AutomationPoint { tick: 0, value: 0 },
                AutomationPoint {
                    tick: AUTOMATION_TICKS_PER_ROW * 2,
                    value: u16::MAX,
                },
            ],
        };
        let mut segments = Vec::new();
        append_lane_segments(
            &mut segments,
            &lane,
            0,
            0,
            0,
            AUTOMATION_TICKS_PER_ROW,
            AUTOMATION_TICKS_PER_ROW * 4,
        );
        assert_eq!(segments[0].start_tick, 0);
        assert_eq!(segments[0].start_value, 32_768);
        assert_eq!(segments[0].end_tick, u64::from(AUTOMATION_TICKS_PER_ROW));
        assert_eq!(segments[0].end_value, u16::MAX);
        assert_eq!(
            lane.value_at(AUTOMATION_TICKS_PER_ROW * 4, AUTOMATION_TICKS_PER_ROW * 4),
            Some(0)
        );
    }

    #[test]
    fn edited_lane_recompiles_to_one_deterministic_replacement_curve() {
        let mut lane = AutomationLane {
            id: 1,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 0,
                controller: 74,
            },
            curve: AutomationCurve::Linear,
            points: vec![
                AutomationPoint { tick: 0, value: 0 },
                AutomationPoint {
                    tick: AUTOMATION_TICKS_PER_ROW,
                    value: 10_000,
                },
            ],
        };
        let before = lane.value_at(AUTOMATION_TICKS_PER_ROW / 2, AUTOMATION_TICKS_PER_ROW * 2);
        lane.points[1].value = 20_000;
        let after = lane.value_at(AUTOMATION_TICKS_PER_ROW / 2, AUTOMATION_TICKS_PER_ROW * 2);
        assert_eq!(before, Some(5_000));
        assert_eq!(after, Some(10_000));
    }

    #[test]
    fn every_managed_instrument_uses_the_same_automation_volume_cc() {
        for engine in crate::preset::BackendKind::ALL {
            let control = if engine == crate::preset::BackendKind::Synthv1 {
                "DCA1_VOLUME"
            } else {
                "instrument_volume"
            };
            let expected = if engine == crate::preset::BackendKind::Synthv1 {
                crate::control::VOLUME_CC
            } else {
                crate::control::INSTRUMENT_VOLUME_CC
            };
            assert_eq!(mapped_control_cc(engine.label(), control), Some(expected));
        }
        assert_eq!(mapped_control_cc("Moj Sint", "mass"), Some(20));
        assert_eq!(mapped_control_cc("Moj Sint", "unstable"), Some(27));
    }
}
