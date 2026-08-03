//! Small non-audible compiler A/B workload over production callback boundaries.

use crate::audio_graph::{default_drum_rack, Monitoring};
use crate::audio_graph_client::managed_graph_definition;
use crate::audio_graph_runtime::GraphPlan;
use crate::drums_host::{discover_kits, DrumEffectStack};
use crate::dsp::StereoFrame;
use crate::final_bus::{BusControls, BusSource, FinalBusMeters, FinalBusProcessor};
use crate::master_strip::{MasterStripControls, MasterStripSettings};
use crate::tempo::Bpm;
use anyhow::{bail, Context, Result};
use shr_drums::{event_queue, load_package, DrumEngine, DrumEvent, KitTuning, ProjectKey};
use std::hint::black_box;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const WARMUP_CALLBACKS: usize = 500;

pub(crate) struct BenchmarkRow {
    pub workload: &'static str,
    pub callback_frames: usize,
    pub callbacks: usize,
    pub mean_microseconds: f64,
    pub median_microseconds: f64,
    pub p95_microseconds: f64,
    pub p99_microseconds: f64,
    pub p999_microseconds: f64,
    pub maximum_microseconds: f64,
    pub mean_deadline_percent: f64,
    pub deadline_misses: usize,
    pub finite: bool,
    pub output_peak: f32,
    pub output_rms: f64,
    pub output_hash: u64,
}

struct OutputEvidence {
    hash: u64,
    peak: f32,
    sum_squares: f64,
    samples: u64,
    finite: bool,
}

impl Default for OutputEvidence {
    fn default() -> Self {
        Self {
            hash: 0xcbf29ce484222325,
            peak: 0.0,
            sum_squares: 0.0,
            samples: 0,
            finite: true,
        }
    }
}

impl OutputEvidence {
    fn add(&mut self, frames: &[StereoFrame]) {
        for frame in frames {
            for sample in [frame.left, frame.right] {
                self.finite &= sample.is_finite();
                self.peak = self.peak.max(sample.abs());
                self.sum_squares += f64::from(sample) * f64::from(sample);
                self.samples += 1;
                for byte in sample.to_bits().to_le_bytes() {
                    self.hash ^= u64::from(byte);
                    self.hash = self.hash.wrapping_mul(0x100000001b3);
                }
            }
        }
    }
}

pub(crate) fn run(kit_directory: &Path, callbacks: usize) -> Result<Vec<BenchmarkRow>> {
    if callbacks < 1_000 {
        bail!("compiler A/B benchmark requires at least 1000 callbacks");
    }
    let kits = discover_kits(kit_directory);
    let kit = kits
        .iter()
        .find(|kit| kit.id == "electronic-house")
        .context("compiler A/B benchmark needs the electronic-house kit")?;
    let mut rows = Vec::new();
    for callback_frames in [64, 128] {
        rows.push(graph_benchmark(
            "graph-dry",
            "dry",
            callback_frames,
            callbacks,
        )?);
        rows.push(graph_benchmark(
            "graph-phase4-full",
            "phase4-full",
            callback_frames,
            callbacks,
        )?);
        for (workload, reverb, delay) in [
            ("drums-dry", false, false),
            ("drums-reverb", true, false),
            ("drums-delay", false, true),
            ("drums-reverb-delay", true, true),
        ] {
            rows.push(drum_benchmark(
                workload,
                kit.path.as_path(),
                callback_frames,
                callbacks,
                reverb,
                delay,
                false,
            )?);
        }
        rows.push(drum_benchmark(
            "drums-melody-final-bus",
            kit.path.as_path(),
            callback_frames,
            callbacks,
            true,
            true,
            true,
        )?);
    }
    Ok(rows)
}

fn graph_benchmark(
    workload: &'static str,
    profile: &str,
    callback_frames: usize,
    callbacks: usize,
) -> Result<BenchmarkRow> {
    let (rack, routing) = crate::effects_routing(profile)?;
    let graph = managed_graph_definition(
        SAMPLE_RATE,
        callback_frames as u32,
        &["benchmark:out-l".into(), "benchmark:out-r".into()],
        &["benchmark:in-l".into(), "benchmark:in-r".into()],
        Monitoring {
            direct: false,
            software: true,
            doubled_path_confirmed: false,
        },
        &rack,
        &routing,
    );
    let mut plan = GraphPlan::compile(&graph).context("compile offline audio graph")?;
    let controls = Arc::new(BusControls::default());
    for source in BusSource::ALL {
        assert!(controls.set_source_gain_db(source, 0.0));
    }
    let strip_controls = Arc::new(
        MasterStripControls::new(SAMPLE_RATE, &MasterStripSettings::default())
            .map_err(anyhow::Error::msg)?,
    );
    let mut final_bus = FinalBusProcessor::new(
        SAMPLE_RATE,
        callback_frames,
        Arc::clone(&controls),
        strip_controls,
        Arc::new(FinalBusMeters::default()),
    )
    .map_err(anyhow::Error::msg)?;
    let source_nodes = plan.source_nodes().to_vec();
    let sink_node = *plan
        .sink_nodes()
        .first()
        .context("offline graph has no sink")?;
    let inputs: [Vec<StereoFrame>; 4] = std::array::from_fn(|source| {
        (0..callback_frames)
            .map(|index| {
                let phase = (source * 131 + index * 17) as f32 * 0.013_7;
                StereoFrame::new(
                    phase.sin() * (0.11 + source as f32 * 0.01),
                    (phase * 1.071 + 0.29).sin() * (0.10 + source as f32 * 0.01),
                )
            })
            .collect()
    });
    let mut final_output = vec![StereoFrame::SILENCE; callback_frames];
    let mut run_callback = |evidence: Option<&mut OutputEvidence>| {
        let started = evidence.is_some().then(Instant::now);
        for ((node, source), input) in source_nodes.iter().zip(BusSource::ALL).zip(inputs.iter()) {
            let buffer = plan
                .source_buffer_mut(*node, callback_frames)
                .expect("validated source buffer");
            buffer.copy_from_slice(input);
            final_bus.process_source(source, buffer);
        }
        black_box(plan.process(callback_frames));
        final_output.copy_from_slice(
            plan.output_buffer(sink_node, callback_frames)
                .expect("validated sink buffer"),
        );
        final_bus.process_final(black_box(&mut final_output));
        let elapsed = started.map(|started| started.elapsed().as_nanos() as u64);
        if let Some(evidence) = evidence {
            evidence.add(&final_output);
        }
        elapsed
    };
    for _ in 0..WARMUP_CALLBACKS {
        run_callback(None);
    }
    let mut durations = Vec::with_capacity(callbacks);
    let mut evidence = OutputEvidence::default();
    for _ in 0..callbacks {
        durations.push(
            run_callback(Some(&mut evidence)).expect("measured graph callback has a duration"),
        );
    }
    Ok(summarize(workload, callback_frames, durations, evidence))
}

fn drum_benchmark(
    workload: &'static str,
    kit_path: &Path,
    callback_frames: usize,
    callbacks: usize,
    reverb: bool,
    delay: bool,
    final_path: bool,
) -> Result<BenchmarkRow> {
    let prepared = load_package(kit_path, ProjectKey::default(), &KitTuning::default())
        .map_err(anyhow::Error::msg)?;
    let (sender, receiver) = event_queue();
    let mut engine =
        DrumEngine::new(SAMPLE_RATE, prepared, receiver).map_err(anyhow::Error::msg)?;
    let mut rack = default_drum_rack("electronic-house", 70)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    rack.effect_mut(70).context("drum reverb missing")?.bypass = !reverb;
    rack.effect_mut(71).context("drum delay missing")?.bypass = !delay;
    let mut effects = DrumEffectStack::compile(&rack, SAMPLE_RATE, callback_frames, Bpm::DEFAULT)?;
    let mut rendered = vec![shr_drums::StereoFrame::SILENCE; callback_frames];
    let mut output = vec![StereoFrame::SILENCE; callback_frames];
    let synth = (0..callback_frames)
        .map(|index| {
            let phase = index as f32 * 0.047;
            StereoFrame::new(phase.sin() * 0.12, (phase + 0.31).sin() * 0.11)
        })
        .collect::<Vec<_>>();
    let mut melody = vec![StereoFrame::SILENCE; callback_frames];
    let controls = Arc::new(BusControls::default());
    for source in BusSource::ALL {
        assert!(controls.set_source_gain_db(source, 0.0));
    }
    let strip_controls = Arc::new(
        MasterStripControls::new(SAMPLE_RATE, &MasterStripSettings::default())
            .map_err(anyhow::Error::msg)?,
    );
    let mut final_bus = FinalBusProcessor::new(
        SAMPLE_RATE,
        callback_frames,
        controls,
        strip_controls,
        Arc::new(FinalBusMeters::default()),
    )
    .map_err(anyhow::Error::msg)?;
    let mut callback_index = 0usize;
    let mut run_callback = |evidence: Option<&mut OutputEvidence>| {
        if callback_index.is_multiple_of(16) {
            sender
                .push(DrumEvent::NoteOn {
                    note: 36,
                    velocity: 112,
                })
                .expect("bounded benchmark event queue");
        }
        if callback_index % 32 == 16 {
            sender
                .push(DrumEvent::NoteOn {
                    note: 38,
                    velocity: 104,
                })
                .expect("bounded benchmark event queue");
        }
        if callback_index.is_multiple_of(4) {
            sender
                .push(DrumEvent::NoteOn {
                    note: 42,
                    velocity: 72,
                })
                .expect("bounded benchmark event queue");
        }
        let started = evidence.is_some().then(Instant::now);
        rendered.fill(shr_drums::StereoFrame::SILENCE);
        engine.process(&mut rendered);
        for (target, source) in output.iter_mut().zip(&rendered) {
            *target = StereoFrame::new(source.left, source.right);
        }
        effects.process(&mut output);
        if final_path {
            final_bus.process_source(BusSource::Drums, &mut output);
            melody.copy_from_slice(&synth);
            final_bus.process_source(BusSource::Synth, &mut melody);
            for (drum, melodic) in output.iter_mut().zip(&melody) {
                drum.left += melodic.left;
                drum.right += melodic.right;
            }
            final_bus.process_final(&mut output);
        }
        let elapsed = started.map(|started| started.elapsed().as_nanos() as u64);
        callback_index += 1;
        if let Some(evidence) = evidence {
            evidence.add(&output);
        }
        elapsed
    };
    for _ in 0..WARMUP_CALLBACKS {
        run_callback(None);
    }
    let mut durations = Vec::with_capacity(callbacks);
    let mut evidence = OutputEvidence::default();
    for _ in 0..callbacks {
        durations.push(
            run_callback(Some(&mut evidence)).expect("measured drum callback has a duration"),
        );
    }
    Ok(summarize(workload, callback_frames, durations, evidence))
}

fn summarize(
    workload: &'static str,
    callback_frames: usize,
    mut durations: Vec<u64>,
    evidence: OutputEvidence,
) -> BenchmarkRow {
    durations.sort_unstable();
    let percentile = |permille: usize| {
        let index = (durations.len() - 1) * permille / 1_000;
        durations[index] as f64 / 1_000.0
    };
    let total_nanoseconds = durations.iter().copied().map(u128::from).sum::<u128>();
    let mean_nanoseconds = total_nanoseconds as f64 / durations.len() as f64;
    let deadline_nanoseconds = callback_frames as f64 / f64::from(SAMPLE_RATE) * 1_000_000_000.0;
    BenchmarkRow {
        workload,
        callback_frames,
        callbacks: durations.len(),
        mean_microseconds: mean_nanoseconds / 1_000.0,
        median_microseconds: percentile(500),
        p95_microseconds: percentile(950),
        p99_microseconds: percentile(990),
        p999_microseconds: percentile(999),
        maximum_microseconds: durations.last().copied().unwrap_or_default() as f64 / 1_000.0,
        mean_deadline_percent: mean_nanoseconds / deadline_nanoseconds * 100.0,
        deadline_misses: durations
            .iter()
            .filter(|duration| **duration as f64 > deadline_nanoseconds)
            .count(),
        finite: evidence.finite,
        output_peak: evidence.peak,
        output_rms: (evidence.sum_squares / evidence.samples.max(1) as f64).sqrt(),
        output_hash: evidence.hash,
    }
}
