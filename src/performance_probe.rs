//! Opt-in live checkpoint diagnostics. No instrumentation in the production callback.
use crate::{
    audio_graph::{InsertRack, ProjectAuxRouting},
    audio_graph_client::FinalBusOwner,
    dsp::MeterSnapshot,
    jack,
};
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    callbacks: AtomicU64,
    xruns: AtomicU64,
    maximum_frames: AtomicU64,
}
unsafe extern "C" fn process(frames: libc::c_uint, arg: *mut libc::c_void) -> libc::c_int {
    let data = unsafe { &*arg.cast::<Counters>() };
    data.frames.fetch_add(frames as u64, Ordering::Relaxed);
    data.callbacks.fetch_add(1, Ordering::Relaxed);
    data.maximum_frames
        .fetch_max(frames as u64, Ordering::Relaxed);
    0
}
unsafe extern "C" fn xrun(arg: *mut libc::c_void) -> libc::c_int {
    unsafe { &*arg.cast::<Counters>() }
        .xruns
        .fetch_add(1, Ordering::Relaxed);
    0
}
pub struct Probe {
    client: jack::Client,
    counters: Box<Counters>,
}
impl Probe {
    pub fn new(name: &str) -> Result<Self> {
        let mut counters = Box::<Counters>::default();
        let mut client = jack::Client::open(name)?;
        let arg = (&mut *counters as *mut Counters).cast();
        // Box remains pinned until deactivation, including error/drop paths.
        unsafe {
            client.set_process_callback(process, arg)?;
            client.set_xrun_callback(xrun, arg)?;
        }
        client.activate()?;
        Ok(Self { client, counters })
    }
    pub fn finish(&mut self) -> serde_json::Value {
        self.client.deactivate();
        serde_json::json!({"frames":self.counters.frames.load(Ordering::Relaxed),
            "callbacks":self.counters.callbacks.load(Ordering::Relaxed),
            "maximum_frames":self.counters.maximum_frames.load(Ordering::Relaxed),
            "xruns":self.counters.xruns.load(Ordering::Relaxed), "rate": self.client.sample_rate()})
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.client.deactivate();
    }
}
#[derive(Default)]
struct Stage {
    peak: f32,
    rms: f32,
    clips: u64,
    non_finite: u64,
}
impl Stage {
    fn sample(&mut self, m: MeterSnapshot) {
        self.peak = self.peak.max(m.peak.left).max(m.peak.right);
        self.rms = self.rms.max(m.rms.left).max(m.rms.right);
        // Published counters are cumulative; never sum repeated reads.
        self.clips = self.clips.max(m.clips);
        self.non_finite = self.non_finite.max(m.non_finite);
    }
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"sampled_peak":self.peak,"sampled_max_rms":self.rms,"over_full_scale_samples":self.clips,"non_finite":self.non_finite})
    }
}
pub struct Stages {
    effects: Vec<(u32, String, Stage, Stage)>,
    input: Stage,
    output: Stage,
    limiter_db: f32,
    true_peak: f32,
}
impl Stages {
    pub fn new(rack: &InsertRack, routing: &ProjectAuxRouting) -> Self {
        Self {
            effects: rack
                .effects
                .iter()
                .chain(routing.buses.iter().flat_map(|b| b.rack.effects.iter()))
                .map(|e| {
                    (
                        e.id,
                        format!("{:?}", e.kind),
                        Stage::default(),
                        Stage::default(),
                    )
                })
                .collect(),
            input: Stage::default(),
            output: Stage::default(),
            limiter_db: 0.0,
            true_peak: -160.0,
        }
    }
    pub fn sample(&mut self, bus: &FinalBusOwner) {
        for (id, _, input, output) in &mut self.effects {
            if let Some(m) = bus.effect_meter(*id) {
                input.sample(m.input);
                output.sample(m.output);
            }
        }
        if let Some(m) = bus.meter() {
            self.input.sample(m.input);
            self.output.sample(m.output);
            self.limiter_db = self.limiter_db.max(m.limiter_gain_reduction_db);
            self.true_peak = self.true_peak.max(m.output_true_peak_dbtp);
        }
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"effects":self.effects.iter().map(|(id, name, input, output)| serde_json::json!({"id":id,"kind":name,"input":input.json(),"output":output.json()})).collect::<Vec<_>>(),
        "strip_input":self.input.json(),"final_output":self.output.json(),"sampled_max_limiter_reduction_db":self.limiter_db,"sampled_max_true_peak_dbtp":self.true_peak})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_counters_are_not_double_counted() {
        let mut stage = Stage::default();
        let m = MeterSnapshot {
            clips: 12,
            non_finite: 2,
            ..Default::default()
        };
        stage.sample(m);
        stage.sample(m);
        assert_eq!(stage.clips, 12);
        assert_eq!(stage.non_finite, 2);
    }
}
