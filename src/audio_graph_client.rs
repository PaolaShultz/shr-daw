//! Disabled-by-default owner for the Phase 1 stereo JACK dry graph.
//!
//! All graph construction and JACK connection changes happen on the owner
//! thread. The callback only copies fixed buffers, runs a preallocated plan,
//! reads atomics, and updates lock-free counters.

use crate::audio_graph::{
    AuxBus, ChannelLayout, Edge, GraphDefinition, InsertRack, Monitoring, Node, NodeKind,
    ProjectAuxRouting, RecordingTap, SendRoute, SinkKind, SourceChain, SourceKind, StereoPorts,
    GRAPH_FORMAT_VERSION,
};
use crate::audio_graph_runtime::{
    CallbackTimingCounters, CallbackTimingSnapshot, GraphPlan, ProcessStatus,
};
use crate::audio_recorder::{FinalMixCapture, FinalMixRecorder, FinalMixRecorderStatus};
use crate::config::{AudioCaptureConfig, AudioGraphConfig};
use crate::dsp::{MeterSnapshot, StereoFrame};
use crate::final_bus::{
    BusControls, BusSource, FinalBusMeterSnapshot, FinalBusMeters, FinalBusProcessor,
};
use crate::jack::{Client as JackClient, Port as JackPort, PortDirection, PortGetBuffer};
use crate::master_strip::{MasterStripControls, MasterStripSettings};
use anyhow::{anyhow, bail, Context, Result};
use libc::{c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

const SOURCE_NODE: u32 = 1;
const LOOP_SOURCE_NODE: u32 = 2;
const INPUT_SOURCE_NODE: u32 = 3;
const DRUM_SOURCE_NODE: u32 = 4;
const FIRST_EFFECT_NODE: u32 = 10;
const FIRST_SEND_NODE: u32 = 30;
const FIRST_AUX_EFFECT_NODE: u32 = 40;
const FIRST_AUX_RETURN_NODE: u32 = 70;
const FIRST_MASTER_EFFECT_NODE: u32 = 80;
const MASTER_NODE: u32 = 90;
const SINK_NODE: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Connection {
    source: String,
    destination: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeKind {
    Connect,
    Disconnect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BoundaryChange {
    kind: ChangeKind,
    connection: Connection,
}

trait BoundaryConnections {
    /// Return true only when the requested operation changed graph state.
    fn connect(&mut self, connection: &Connection) -> Result<bool>;
    fn disconnect(&mut self, connection: &Connection) -> Result<bool>;
}

impl BoundaryConnections for JackClient {
    fn connect(&mut self, connection: &Connection) -> Result<bool> {
        self.ensure_connection(&connection.source, &connection.destination)
    }

    fn disconnect(&mut self, connection: &Connection) -> Result<bool> {
        self.remove_connection(&connection.source, &connection.destination)
    }
}

fn apply_transaction(
    connections: &mut impl BoundaryConnections,
    changes: &[BoundaryChange],
) -> Result<()> {
    let mut applied = Vec::with_capacity(changes.len());
    for change in changes {
        let result = match change.kind {
            ChangeKind::Connect => connections.connect(&change.connection),
            ChangeKind::Disconnect => connections.disconnect(&change.connection),
        };
        match result {
            Ok(true) => applied.push(change.clone()),
            Ok(false) => {}
            Err(error) => {
                let rollback_error = rollback(connections, &applied).err();
                return match rollback_error {
                    Some(rollback) => Err(anyhow!(
                        "audio boundary change failed: {error:#}; rollback failed: {rollback:#}"
                    )),
                    None => Err(error.context("audio boundary change rolled back")),
                };
            }
        }
    }
    Ok(())
}

fn rollback(connections: &mut impl BoundaryConnections, applied: &[BoundaryChange]) -> Result<()> {
    let mut first_error = None;
    for change in applied.iter().rev() {
        let result = match change.kind {
            ChangeKind::Connect => connections.disconnect(&change.connection),
            ChangeKind::Disconnect => connections.connect(&change.connection),
        };
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn sync_optional_if_available(
    connections: &mut impl BoundaryConnections,
    source: &OptionalSourceRoutes,
    available_ports: &[String],
) -> Result<bool> {
    if !source
        .ports
        .iter()
        .all(|port| available_ports.iter().any(|candidate| candidate == port))
    {
        return Ok(false);
    }
    apply_transaction(
        connections,
        &BoundaryRoutes::source_connection_changes(source),
    )?;
    Ok(true)
}

struct BoundaryRoutes {
    required_graph: Vec<Connection>,
    optional_sources: Vec<OptionalSourceRoutes>,
    destinations: [String; 2],
    loop_destinations: [String; 2],
    graph_inputs: [String; 8],
    live_source_ports: [String; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OptionalSourceRoutes {
    source: BusSource,
    ports: [String; 2],
    direct: [Connection; 2],
    graph: [Connection; 2],
}

impl BoundaryRoutes {
    #[cfg(test)]
    fn direct_connection_changes(&self) -> Vec<BoundaryChange> {
        self.optional_sources
            .iter()
            .flat_map(|source| source.direct.iter())
            .cloned()
            .map(|connection| BoundaryChange {
                kind: ChangeKind::Connect,
                connection,
            })
            .collect()
    }

    fn required_connection_changes(&self) -> Vec<BoundaryChange> {
        self.required_graph
            .iter()
            .cloned()
            .map(|connection| BoundaryChange {
                kind: ChangeKind::Connect,
                connection,
            })
            .collect()
    }

    fn source_connection_changes(source: &OptionalSourceRoutes) -> Vec<BoundaryChange> {
        source
            .graph
            .iter()
            .cloned()
            .map(|connection| BoundaryChange {
                kind: ChangeKind::Connect,
                connection,
            })
            .chain(
                source
                    .direct
                    .iter()
                    .cloned()
                    .map(|connection| BoundaryChange {
                        kind: ChangeKind::Disconnect,
                        connection,
                    }),
            )
            .collect()
    }

    fn source_routes(&self, source: BusSource) -> Option<&OptionalSourceRoutes> {
        self.optional_sources
            .iter()
            .find(|routes| routes.source == source)
    }

    fn source_routes_mut(&mut self, source: BusSource) -> Option<&mut OptionalSourceRoutes> {
        self.optional_sources
            .iter_mut()
            .find(|routes| routes.source == source)
    }
}

struct CallbackData {
    plan: GraphPlan,
    inputs: [*mut JackPort; 8],
    input_port_ids: [u32; 8],
    output_left: *mut JackPort,
    output_right: *mut JackPort,
    port_get_buffer: PortGetBuffer,
    sample_rate: u32,
    armed: AtomicBool,
    client_lost: AtomicBool,
    source_lost: AtomicBool,
    input_monitoring: AtomicBool,
    timing: CallbackTimingCounters,
    final_bus: FinalBusProcessor,
    final_capture: FinalMixCapture,
    final_buffer: Box<[StereoFrame]>,
}

// JACK owns callback scheduling, while the box itself remains pinned and is
// reclaimed only after deactivation on the non-real-time owner thread.
unsafe impl Send for CallbackData {}

pub(crate) struct OwnedAudioGraph {
    jack: JackClient,
    callback: Box<CallbackData>,
    routes: BoundaryRoutes,
    controls: std::sync::Arc<BusControls>,
    strip_controls: std::sync::Arc<MasterStripControls>,
    meters: std::sync::Arc<FinalBusMeters>,
    final_recorder: FinalMixRecorder,
    monitoring: Monitoring,
}

#[derive(Default)]
pub(crate) struct FinalBusOwner {
    graph: Option<OwnedAudioGraph>,
    last_recording: FinalMixRecorderStatus,
    fallback: Option<String>,
    #[cfg(test)]
    controls_override: Option<std::sync::Arc<BusControls>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct EffectMeterSnapshot {
    pub input: MeterSnapshot,
    pub output: MeterSnapshot,
    pub gain_reduction_db: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AuxMeterSnapshot {
    pub output: MeterSnapshot,
}

pub(crate) struct PerformanceBusPorts {
    pub synth: Option<[String; 2]>,
    pub loop_player: Option<[String; 2]>,
    pub drums: Option<[String; 2]>,
    pub live_input: [String; 2],
    pub playback: [String; 2],
    pub loop_direct_playback: [String; 2],
}

impl FinalBusOwner {
    pub(crate) fn active(&self) -> bool {
        self.graph.is_some()
    }

    pub(crate) fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    // These arguments are distinct ownership boundaries; grouping them would
    // obscure which live state the final bus borrows versus copies.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn activate(
        &mut self,
        config: &crate::config::RuntimeConfig,
        managed_client_name: Option<&str>,
        loop_ports: Option<[String; 2]>,
        drum_ports: Option<[String; 2]>,
        rack: &InsertRack,
        aux_routing: &ProjectAuxRouting,
        master_strip: &MasterStripSettings,
        input_monitoring: bool,
    ) -> Result<bool> {
        if let Some(graph) = self.graph.as_mut() {
            if input_monitoring {
                let available = crate::engine::jack_ports();
                if !graph.retry_required_connections(&available)? {
                    bail!("configured final-bus input is offline; MON ON left monitoring off");
                }
            }
            graph.set_input_monitoring(input_monitoring)?;
            self.sync_sources(managed_client_name, loop_ports, drum_ports)?;
            self.fallback = None;
            return Ok(false);
        }
        let available = crate::engine::jack_ports();
        let input = config
            .audio_graph
            .input
            .as_ref()
            .or_else(|| config.capture.inputs.first())
            .context("final bus needs one configured stereo JACK input")?;
        let live_input = [input.left_port.clone(), input.right_port.clone()];
        for port in &live_input {
            if !available.iter().any(|candidate| candidate == port) {
                bail!(
                    "configured final-bus input {port:?} is offline; no nearby JACK port is substituted"
                );
            }
        }
        let resolved_audio = config.resolve_audio_route(&available);
        let playback: [String; 2] = resolved_audio
            .outputs
            .try_into()
            .map_err(|_| anyhow!("owned graph requires exactly two configured main outputs"))?;
        let loop_direct_playback: [String; 2] =
            config.loop_player.outputs.clone().try_into().map_err(|_| {
                anyhow!("final bus requires exactly two configured loop.output routes")
            })?;
        let synth = managed_client_name.and_then(|client_name| {
            crate::engine::resolve_managed_audio_outputs(client_name, available.clone()).ok()
        });
        let graph = OwnedAudioGraph::start_with_routing(
            &config.audio_graph,
            PerformanceBusPorts {
                synth,
                loop_player: loop_ports,
                drums: drum_ports,
                live_input,
                playback,
                loop_direct_playback,
            },
            &config.capture,
            rack,
            aux_routing,
            master_strip,
            input_monitoring,
            &available,
        )?;
        self.graph = Some(graph);
        self.fallback = resolved_audio.notice;
        Ok(true)
    }

    pub(crate) fn sync_sources(
        &mut self,
        managed_client_name: Option<&str>,
        loop_ports: Option<[String; 2]>,
        drum_ports: Option<[String; 2]>,
    ) -> Result<()> {
        let Some(graph) = self.graph.as_mut() else {
            return Ok(());
        };
        let available = crate::engine::jack_ports();
        let synth = managed_client_name.and_then(|client_name| {
            crate::engine::resolve_managed_audio_outputs(client_name, available.clone()).ok()
        });
        graph.sync_optional_source(BusSource::Synth, synth, &available)?;
        graph.sync_optional_source(BusSource::Loop, loop_ports, &available)?;
        graph.sync_optional_source(BusSource::Drums, drum_ports, &available)?;
        Ok(())
    }

    pub(crate) fn set_input_monitoring(&mut self, enabled: bool) -> Result<()> {
        self.graph
            .as_mut()
            .context("final bus is inactive")?
            .set_input_monitoring(enabled)
    }

    pub(crate) fn effect_meter(&self, effect_id: u32) -> Option<EffectMeterSnapshot> {
        self.graph.as_ref()?.effect_meter(effect_id)
    }

    pub(crate) fn master_meter(&self) -> Option<AuxMeterSnapshot> {
        self.graph.as_ref()?.master_meter()
    }

    pub(crate) fn meter(&self) -> Option<FinalBusMeterSnapshot> {
        Some(self.graph.as_ref()?.final_bus_meter())
    }

    pub(crate) fn controls(&self) -> Option<std::sync::Arc<BusControls>> {
        if let Some(graph) = self.graph.as_ref() {
            return Some(graph.bus_controls());
        }
        #[cfg(test)]
        {
            self.controls_override.as_ref().map(std::sync::Arc::clone)
        }
        #[cfg(not(test))]
        None
    }

    #[cfg(test)]
    pub(crate) fn set_controls_override(&mut self, controls: std::sync::Arc<BusControls>) {
        self.controls_override = Some(controls);
    }

    pub(crate) fn apply_master_strip(&self, settings: &MasterStripSettings) -> Result<bool> {
        let Some(graph) = self.graph.as_ref() else {
            return Ok(false);
        };
        graph
            .master_strip_controls()
            .apply(settings)
            .map_err(anyhow::Error::msg)?;
        Ok(true)
    }

    pub(crate) fn reset_master_strip_loudness(&self) -> bool {
        let Some(graph) = self.graph.as_ref() else {
            return false;
        };
        graph.master_strip_controls().reset_loudness();
        true
    }

    pub(crate) fn publish_routing(
        &mut self,
        rack: &InsertRack,
        aux_routing: &ProjectAuxRouting,
    ) -> Result<bool> {
        let Some(graph) = self.graph.as_mut() else {
            return Ok(false);
        };
        graph.publish_routing(rack, aux_routing)?;
        Ok(true)
    }

    pub(crate) fn recording_status(&mut self) -> FinalMixRecorderStatus {
        if let Some(graph) = self.graph.as_mut() {
            self.last_recording = graph.final_recording_status();
        }
        self.last_recording.clone()
    }

    pub(crate) fn recording_active(&self) -> bool {
        self.graph
            .as_ref()
            .is_some_and(OwnedAudioGraph::final_recording_active)
    }

    pub(crate) fn start_recording(&mut self, name: Option<&str>) -> Result<()> {
        let graph = self
            .graph
            .as_mut()
            .context("final mix unavailable · owned graph is inactive")?;
        graph.start_final_recording(name)?;
        self.last_recording = graph.final_recording_status();
        Ok(())
    }

    pub(crate) fn stop_recording(&mut self) -> Result<()> {
        let graph = self
            .graph
            .as_mut()
            .context("final mix unavailable · owned graph is inactive")?;
        let result = graph.stop_final_recording();
        self.last_recording = graph.final_recording_status();
        result
    }

    pub(crate) fn sample_rate(&self) -> Option<u32> {
        self.graph.as_ref().map(OwnedAudioGraph::sample_rate)
    }

    pub(crate) fn poll(&mut self) -> Option<String> {
        if self
            .graph
            .as_ref()
            .is_some_and(OwnedAudioGraph::client_lost)
        {
            let (_, restored) = self.deactivate()?;
            let message = match restored {
                Ok(()) => "AUDIO GRAPH LOST · exact direct routes restored".to_owned(),
                Err(error) => {
                    format!("AUDIO GRAPH LOST · direct restore unavailable: {error:#}")
                }
            };
            self.fallback = Some(message.clone());
            return Some(message);
        }
        if self
            .graph
            .as_ref()
            .is_some_and(OwnedAudioGraph::source_lost)
        {
            let available = crate::engine::jack_ports();
            return match self
                .graph
                .as_mut()
                .expect("source-loss check retained graph")
                .retry_required_connections(&available)
            {
                Ok(true) => Some("INPUT MONITOR RESTORED".into()),
                Ok(false) => Some("INPUT OFFLINE · reconnect configured capture".into()),
                Err(error) => Some(format!("INPUT MONITOR RETRY FAILED · {error:#}")),
            };
        }
        None
    }

    pub(crate) fn deactivate(&mut self) -> Option<(CallbackTimingSnapshot, Result<()>)> {
        let mut graph = self.graph.take()?;
        let restored = graph.restore_direct();
        self.last_recording = graph.final_recording_status();
        let timing = graph.timing();
        drop(graph);
        Some((timing, restored))
    }
}

impl OwnedAudioGraph {
    pub(crate) fn sample_rate(&self) -> u32 {
        self.callback.sample_rate
    }

    // The constructor mirrors the explicit graph transaction inputs and keeps
    // route preflight independent from live JACK mutation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_with_routing(
        config: &AudioGraphConfig,
        ports: PerformanceBusPorts,
        recording: &AudioCaptureConfig,
        rack: &InsertRack,
        aux_routing: &ProjectAuxRouting,
        master_strip: &MasterStripSettings,
        input_monitoring: bool,
        available_ports: &[String],
    ) -> Result<Self> {
        let PerformanceBusPorts {
            synth: source_ports,
            loop_player: loop_source_ports,
            drums: drum_source_ports,
            live_input: live_source_ports,
            playback: destinations,
            loop_direct_playback: loop_destinations,
        } = ports;
        if let Some(ports) = source_ports.as_ref() {
            validate_stereo_boundary(ports, "managed-engine source")?;
        }
        if let Some(ports) = loop_source_ports.as_ref() {
            validate_stereo_boundary(ports, "owned WAV loop source")?;
        }
        if let Some(ports) = drum_source_ports.as_ref() {
            validate_stereo_boundary(ports, "SHR Drums source")?;
        }
        validate_stereo_boundary(&live_source_ports, "configured stereo input")?;
        validate_stereo_boundary(&destinations, "main output")?;
        validate_stereo_boundary(&loop_destinations, "loop direct output")?;
        if input_monitoring && config.input_direct_monitoring && !config.confirm_doubled_monitoring
        {
            bail!("configured stereo input has interface direct monitoring enabled; confirm the deliberate doubled monitoring path or disable direct monitoring before software monitoring");
        }

        let mut jack = JackClient::open(&config.client_name).context("open owned audio graph")?;
        let sample_rate = jack.sample_rate();
        if sample_rate == 0 {
            bail!("JACK reported a zero sample rate");
        }
        let monitoring = Monitoring {
            direct: config.input_direct_monitoring,
            software: input_monitoring,
            doubled_path_confirmed: config.confirm_doubled_monitoring,
        };
        let definition = managed_graph_definition(
            sample_rate,
            config.maximum_callback_frames,
            &destinations,
            &live_source_ports,
            monitoring,
            rack,
            aux_routing,
        );
        let plan = GraphPlan::compile(&definition).context("compile managed audio graph")?;

        let inputs = [
            jack.register_audio_port("managed_in_l", PortDirection::Input)?,
            jack.register_audio_port("managed_in_r", PortDirection::Input)?,
            jack.register_audio_port("loop_in_l", PortDirection::Input)?,
            jack.register_audio_port("loop_in_r", PortDirection::Input)?,
            jack.register_audio_port("stereo_in_l", PortDirection::Input)?,
            jack.register_audio_port("stereo_in_r", PortDirection::Input)?,
            jack.register_audio_port("drums_in_l", PortDirection::Input)?,
            jack.register_audio_port("drums_in_r", PortDirection::Input)?,
        ];
        let input_port_ids = [
            jack.port_id(inputs[0])?,
            jack.port_id(inputs[1])?,
            jack.port_id(inputs[2])?,
            jack.port_id(inputs[3])?,
            jack.port_id(inputs[4])?,
            jack.port_id(inputs[5])?,
            jack.port_id(inputs[6])?,
            jack.port_id(inputs[7])?,
        ];
        let output_left = jack.register_audio_port("main_out_l", PortDirection::Output)?;
        let output_right = jack.register_audio_port("main_out_r", PortDirection::Output)?;
        let graph_port_names = [
            jack.port_name_string(inputs[0])?,
            jack.port_name_string(inputs[1])?,
            jack.port_name_string(inputs[2])?,
            jack.port_name_string(inputs[3])?,
            jack.port_name_string(inputs[4])?,
            jack.port_name_string(inputs[5])?,
            jack.port_name_string(inputs[6])?,
            jack.port_name_string(inputs[7])?,
            jack.port_name_string(output_left)?,
            jack.port_name_string(output_right)?,
        ];
        let required_graph = vec![
            connection(&live_source_ports[0], &graph_port_names[4]),
            connection(&live_source_ports[1], &graph_port_names[5]),
            connection(&graph_port_names[8], &destinations[0]),
            connection(&graph_port_names[9], &destinations[1]),
        ];
        let mut optional_sources = Vec::new();
        if let Some(source_ports) = source_ports {
            optional_sources.push(optional_source_routes(
                BusSource::Synth,
                source_ports,
                &destinations,
                [&graph_port_names[0], &graph_port_names[1]],
            ));
        }
        if let Some(loop_source_ports) = loop_source_ports {
            optional_sources.push(optional_source_routes(
                BusSource::Loop,
                loop_source_ports,
                &loop_destinations,
                [&graph_port_names[2], &graph_port_names[3]],
            ));
        }
        if let Some(drums) = drum_source_ports {
            optional_sources.push(optional_source_routes(
                BusSource::Drums,
                drums,
                &destinations,
                [&graph_port_names[6], &graph_port_names[7]],
            ));
        }
        let routes = BoundaryRoutes {
            required_graph,
            optional_sources,
            destinations: destinations.clone(),
            loop_destinations: loop_destinations.clone(),
            graph_inputs: std::array::from_fn(|index| graph_port_names[index].clone()),
            live_source_ports: live_source_ports.clone(),
        };
        let controls = initial_bus_controls(input_monitoring);
        let strip_controls = std::sync::Arc::new(
            MasterStripControls::new(sample_rate, master_strip)
                .map_err(anyhow::Error::msg)
                .context("prepare MASTER STRIP controls")?,
        );
        let meters = std::sync::Arc::new(FinalBusMeters::default());
        let final_bus = FinalBusProcessor::new(
            sample_rate,
            config.maximum_callback_frames as usize,
            std::sync::Arc::clone(&controls),
            std::sync::Arc::clone(&strip_controls),
            std::sync::Arc::clone(&meters),
        )
        .map_err(anyhow::Error::msg)
        .context("prepare final performance bus")?;
        let final_recorder = FinalMixRecorder::new(
            recording.directory.clone(),
            sample_rate,
            recording.ring_frames,
            config.maximum_callback_frames as usize,
        )?;
        let final_capture = final_recorder.capture_handle();
        let mut callback = Box::new(CallbackData {
            plan,
            inputs,
            input_port_ids,
            output_left,
            output_right,
            port_get_buffer: jack.port_get_buffer(),
            sample_rate,
            armed: AtomicBool::new(false),
            client_lost: AtomicBool::new(false),
            source_lost: AtomicBool::new(false),
            input_monitoring: AtomicBool::new(input_monitoring),
            timing: CallbackTimingCounters::default(),
            final_bus,
            final_capture,
            final_buffer: vec![StereoFrame::SILENCE; config.maximum_callback_frames as usize]
                .into_boxed_slice(),
        });
        let callback_pointer = ((&mut *callback) as *mut CallbackData).cast();
        // SAFETY: callback remains boxed until after explicit JACK deactivation.
        unsafe {
            jack.set_process_callback(process_callback, callback_pointer)?;
            jack.set_shutdown_callback(shutdown_callback, callback_pointer);
            jack.set_xrun_callback(xrun_callback, callback_pointer)?;
            jack.set_port_connect_callback(port_connect_callback, callback_pointer)?;
        }
        jack.activate().context("activate owned audio graph")?;
        // Re-establish the conservative route through JACK's checked API even
        // if the legacy jack_connect helper was unavailable or raced startup.
        if let Err(error) = apply_transaction(&mut jack, &routes.required_connection_changes()) {
            jack.deactivate();
            return Err(error.context("activate owned graph boundary"));
        }
        for source in &routes.optional_sources {
            if let Err(error) = sync_optional_if_available(&mut jack, source, available_ports) {
                jack.deactivate();
                return Err(error.context("activate optional final-bus source boundary"));
            }
        }
        // The callback samples this once per block. All graph connections are
        // ready and every owned direct source link is gone before output is
        // published.
        callback.armed.store(true, Ordering::Release);
        Ok(Self {
            jack,
            callback,
            routes,
            controls,
            strip_controls,
            meters,
            final_recorder,
            monitoring,
        })
    }

    pub(crate) fn set_input_monitoring(&mut self, enabled: bool) -> Result<()> {
        if enabled && self.monitoring.direct && !self.monitoring.doubled_path_confirmed {
            bail!("interface direct monitor is declared active; disable it before SHR software monitoring or deliberately confirm the doubled path");
        }
        self.monitoring.software = enabled;
        self.callback
            .input_monitoring
            .store(enabled, Ordering::Release);
        self.controls.set_source_muted(BusSource::Input, !enabled);
        Ok(())
    }

    pub(crate) fn sync_optional_source(
        &mut self,
        source: BusSource,
        ports: Option<[String; 2]>,
        available_ports: &[String],
    ) -> Result<()> {
        if source == BusSource::Input {
            bail!("Input is the required final-bus source, not an optional source");
        }
        if let Some(ports) = ports {
            validate_stereo_boundary(&ports, source.label())?;
            let destinations = if source == BusSource::Loop {
                self.routes.loop_destinations.clone()
            } else {
                self.routes.destinations.clone()
            };
            let input_indices = match source {
                BusSource::Synth => [0, 1],
                BusSource::Loop => [2, 3],
                BusSource::Drums => [6, 7],
                BusSource::Input => unreachable!(),
            };
            let graph_inputs = [
                self.routes.graph_inputs[input_indices[0]].as_str(),
                self.routes.graph_inputs[input_indices[1]].as_str(),
            ];
            let replacement = optional_source_routes(source, ports, &destinations, graph_inputs);
            if let Some(existing) = self.routes.source_routes_mut(source) {
                if existing.ports != replacement.ports {
                    *existing = replacement;
                }
            } else {
                self.routes.optional_sources.push(replacement);
            }
        }
        let Some(routes) = self.routes.source_routes(source).cloned() else {
            return Ok(());
        };
        sync_optional_if_available(&mut self.jack, &routes, available_ports)
            .with_context(|| format!("connect optional {} source", source.label()))?;
        Ok(())
    }

    pub(crate) fn retry_required_connections(
        &mut self,
        available_ports: &[String],
    ) -> Result<bool> {
        if !self
            .routes
            .live_source_ports
            .iter()
            .all(|port| available_ports.iter().any(|candidate| candidate == port))
        {
            return Ok(false);
        }
        apply_transaction(&mut self.jack, &self.routes.required_connection_changes())
            .context("restore exact Input monitoring boundary")?;
        self.callback.source_lost.store(false, Ordering::Release);
        Ok(true)
    }

    pub(crate) fn client_lost(&self) -> bool {
        self.callback.client_lost.load(Ordering::Acquire)
    }

    pub(crate) fn source_lost(&self) -> bool {
        self.callback.source_lost.load(Ordering::Acquire)
    }

    pub(crate) fn timing(&self) -> CallbackTimingSnapshot {
        self.callback.timing.snapshot()
    }

    pub(crate) fn effect_meter(&self, effect_id: u32) -> Option<EffectMeterSnapshot> {
        let handles = self.callback.plan.effect_meters_by_id(effect_id)?;
        Some(EffectMeterSnapshot {
            input: handles.input.load(),
            output: handles.output.load(),
            gain_reduction_db: handles.gain_reduction.map(|meter| meter.load()),
        })
    }

    pub(crate) fn master_meter(&self) -> Option<AuxMeterSnapshot> {
        Some(AuxMeterSnapshot {
            output: self.meters.snapshot().output,
        })
    }

    pub(crate) fn final_bus_meter(&self) -> FinalBusMeterSnapshot {
        self.meters.snapshot()
    }

    pub(crate) fn bus_controls(&self) -> std::sync::Arc<BusControls> {
        std::sync::Arc::clone(&self.controls)
    }

    pub(crate) fn master_strip_controls(&self) -> std::sync::Arc<MasterStripControls> {
        std::sync::Arc::clone(&self.strip_controls)
    }

    pub(crate) fn final_recording_status(&mut self) -> FinalMixRecorderStatus {
        self.final_recorder.status()
    }

    pub(crate) fn final_recording_active(&self) -> bool {
        self.final_recorder.is_recording()
    }

    pub(crate) fn start_final_recording(&mut self, name: Option<&str>) -> Result<()> {
        self.final_recorder.start(name)
    }

    pub(crate) fn stop_final_recording(&mut self) -> Result<()> {
        self.final_recorder.request_stop();
        self.final_recorder.finish_stop()
    }

    /// Publish a validated structural rack change while transport and all
    /// recording are stopped. JACK callback execution is joined before the
    /// plan is mutated; compatible effect IDs retain their runtime state.
    pub(crate) fn publish_routing(
        &mut self,
        rack: &InsertRack,
        aux_routing: &ProjectAuxRouting,
    ) -> Result<()> {
        let destinations = self.routes.destinations.clone();
        let definition = managed_graph_definition(
            self.callback.sample_rate,
            self.callback.plan.maximum_frames() as u32,
            &destinations,
            &self.routes.live_source_ports,
            self.monitoring,
            rack,
            aux_routing,
        );
        definition
            .validate()
            .map_err(|error| anyhow!(error.to_string()))?;
        self.callback.armed.store(false, Ordering::Release);
        self.jack.deactivate();
        if let Err(error) = self.callback.plan.reconfigure(&definition) {
            if self.jack.activate().is_ok() {
                self.callback.armed.store(true, Ordering::Release);
            } else {
                let _ = self.restore_direct();
            }
            return Err(anyhow!(error.to_string()).context("compile replacement audio rack"));
        }
        self.callback.final_bus.reset();
        if let Err(error) = self.jack.activate() {
            let _ = self.restore_direct();
            return Err(error.context("reactivate audio graph after rack publication"));
        }
        if let Err(error) =
            apply_transaction(&mut self.jack, &self.routes.required_connection_changes())
        {
            let _ = self.restore_direct();
            return Err(error.context("restore audio graph boundary after rack publication"));
        }
        for routes in self.routes.optional_sources.clone() {
            let _ = apply_transaction(
                &mut self.jack,
                &BoundaryRoutes::source_connection_changes(&routes),
            );
        }
        self.callback.armed.store(true, Ordering::Release);
        Ok(())
    }

    /// Restore available optional-source direct links best-effort. This runs
    /// only on the non-real-time owner thread, including client-loss recovery.
    pub(crate) fn restore_direct(&mut self) -> Result<()> {
        self.callback.armed.store(false, Ordering::Release);
        // Join the callback before creating either direct link. A callback
        // that sampled the previous publish flag can therefore never overlap
        // the restored dry path for even one block.
        self.jack.deactivate();
        let recorder_result = self.final_recorder.stop_after_deactivate();
        let mut first_error = None;
        let available_ports = crate::engine::jack_ports();
        for source in &self.routes.optional_sources {
            for connection in &source.direct {
                if let Err(error) = self
                    .jack
                    .ensure_connection(&connection.source, &connection.destination)
                {
                    // Optional sources may have disappeared. Their absent
                    // ports are silence, not a shutdown failure.
                    if source
                        .ports
                        .iter()
                        .all(|port| available_ports.iter().any(|candidate| candidate == port))
                    {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        if let Err(error) = recorder_result {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for OwnedAudioGraph {
    fn drop(&mut self) {
        let _ = self.restore_direct();
        // `callback` is still alive here and is dropped only after this method.
    }
}

fn connection(source: &str, destination: &str) -> Connection {
    Connection {
        source: source.into(),
        destination: destination.into(),
    }
}

fn optional_source_routes(
    source: BusSource,
    ports: [String; 2],
    destinations: &[String; 2],
    graph_inputs: [&str; 2],
) -> OptionalSourceRoutes {
    OptionalSourceRoutes {
        source,
        direct: [
            connection(&ports[0], &destinations[0]),
            connection(&ports[1], &destinations[1]),
        ],
        graph: [
            connection(&ports[0], graph_inputs[0]),
            connection(&ports[1], graph_inputs[1]),
        ],
        ports,
    }
}

fn initial_bus_controls(input_monitoring: bool) -> std::sync::Arc<BusControls> {
    let controls = std::sync::Arc::new(BusControls::default());
    controls.set_source_muted(BusSource::Input, !input_monitoring);
    controls
}

fn validate_stereo_boundary(ports: &[String; 2], description: &str) -> Result<()> {
    if ports.iter().any(|port| port.trim().is_empty()) {
        bail!("{description} contains an empty JACK port name");
    }
    if ports[0] == ports[1] {
        bail!("{description} JACK ports are ambiguous");
    }
    Ok(())
}

pub(crate) fn managed_graph_definition(
    sample_rate: u32,
    maximum_callback_frames: u32,
    destinations: &[String; 2],
    live_source_ports: &[String; 2],
    monitoring: Monitoring,
    rack: &InsertRack,
    aux_routing: &ProjectAuxRouting,
) -> GraphDefinition {
    let mut nodes = vec![
        Node {
            id: SOURCE_NODE,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Source {
                source: SourceKind::ManagedEngine,
            },
        },
        Node {
            id: LOOP_SOURCE_NODE,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Source {
                source: SourceKind::LoopPlayer,
            },
        },
        Node {
            id: INPUT_SOURCE_NODE,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Source {
                source: SourceKind::LiveInput {
                    ports: StereoPorts {
                        left: live_source_ports[0].clone(),
                        right: live_source_ports[1].clone(),
                    },
                },
            },
        },
        Node {
            id: DRUM_SOURCE_NODE,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Source {
                source: SourceKind::InternalDrums,
            },
        },
    ];
    let mut edges = Vec::new();
    let mut previous = SOURCE_NODE;
    for (index, effect_id) in rack.order.iter().copied().enumerate() {
        let node_id = FIRST_EFFECT_NODE + index as u32;
        nodes.push(Node {
            id: node_id,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Processor { effect_id },
        });
        edges.push(Edge {
            id: edges.len() as u32 + 1,
            from: previous,
            to: node_id,
        });
        previous = node_id;
    }
    nodes.push(Node {
        id: MASTER_NODE,
        layout: ChannelLayout::Stereo,
        kind: NodeKind::StereoMixer,
    });
    edges.push(Edge {
        id: edges.len() as u32 + 1,
        from: previous,
        to: MASTER_NODE,
    });
    for source in [LOOP_SOURCE_NODE, INPUT_SOURCE_NODE, DRUM_SOURCE_NODE] {
        edges.push(Edge {
            id: edges.len() as u32 + 1,
            from: source,
            to: MASTER_NODE,
        });
    }

    let mut effects = rack.effects.clone();
    let mut aux_buses = Vec::new();
    let mut sends = Vec::new();
    let mut aux_effect_node = FIRST_AUX_EFFECT_NODE;
    for (bus_index, bus) in aux_routing.buses.iter().enumerate() {
        effects.extend(bus.rack.effects.iter().cloned());
        aux_buses.push(AuxBus {
            id: bus.id,
            effects: bus.rack.order.clone(),
            return_gain_db: bus.return_gain_db,
        });
        let send = aux_routing.sends.iter().find(|send| send.aux_id == bus.id);
        let mut aux_previous = None;
        if let Some(send) = send {
            let send_node = FIRST_SEND_NODE + bus_index as u32;
            nodes.push(Node {
                id: send_node,
                layout: ChannelLayout::Stereo,
                kind: NodeKind::SendTap {
                    aux_id: bus.id,
                    source_node: SOURCE_NODE,
                },
            });
            edges.push(Edge {
                id: edges.len() as u32 + 1,
                from: match send.point {
                    crate::audio_graph::SendPoint::PreInsert => SOURCE_NODE,
                    crate::audio_graph::SendPoint::PostInsert => previous,
                },
                to: send_node,
            });
            sends.push(SendRoute {
                source_node: SOURCE_NODE,
                aux_id: bus.id,
                level_db: send.level_db,
                point: send.point,
            });
            aux_previous = Some(send_node);
        }
        for effect_id in bus.rack.order.iter().copied() {
            let node_id = aux_effect_node;
            aux_effect_node += 1;
            nodes.push(Node {
                id: node_id,
                layout: ChannelLayout::Stereo,
                kind: NodeKind::Processor { effect_id },
            });
            if let Some(from) = aux_previous {
                edges.push(Edge {
                    id: edges.len() as u32 + 1,
                    from,
                    to: node_id,
                });
            }
            aux_previous = Some(node_id);
        }
        let return_node = FIRST_AUX_RETURN_NODE + bus_index as u32;
        nodes.push(Node {
            id: return_node,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::AuxReturn { aux_id: bus.id },
        });
        if let Some(from) = aux_previous {
            edges.push(Edge {
                id: edges.len() as u32 + 1,
                from,
                to: return_node,
            });
        }
        edges.push(Edge {
            id: edges.len() as u32 + 1,
            from: return_node,
            to: MASTER_NODE,
        });
    }

    let mut master_previous = MASTER_NODE;
    for (index, effect_id) in aux_routing.master_rack.order.iter().copied().enumerate() {
        let node_id = FIRST_MASTER_EFFECT_NODE + index as u32;
        nodes.push(Node {
            id: node_id,
            layout: ChannelLayout::Stereo,
            kind: NodeKind::Processor { effect_id },
        });
        edges.push(Edge {
            id: edges.len() as u32 + 1,
            from: master_previous,
            to: node_id,
        });
        master_previous = node_id;
    }
    effects.extend(aux_routing.master_rack.effects.iter().cloned());

    nodes.push(Node {
        id: SINK_NODE,
        layout: ChannelLayout::Stereo,
        kind: NodeKind::Sink {
            sink: SinkKind::MainPlayback {
                ports: StereoPorts {
                    left: destinations[0].clone(),
                    right: destinations[1].clone(),
                },
            },
        },
    });
    edges.push(Edge {
        id: edges.len() as u32 + 1,
        from: master_previous,
        to: SINK_NODE,
    });
    GraphDefinition {
        format_version: GRAPH_FORMAT_VERSION,
        enabled: true,
        sample_rate,
        maximum_callback_frames,
        nodes,
        edges,
        effects,
        source_chains: vec![SourceChain {
            source_node: SOURCE_NODE,
            effects: rack.order.clone(),
        }],
        master_chain: aux_routing.master_rack.order.clone(),
        aux_buses,
        sends,
        monitoring,
        recording_tap: RecordingTap::PostMaster,
    }
}

#[cfg(test)]
fn dry_graph_definition(
    sample_rate: u32,
    maximum_callback_frames: u32,
    destinations: &[String; 2],
) -> GraphDefinition {
    managed_graph_definition(
        sample_rate,
        maximum_callback_frames,
        destinations,
        &["input:l".into(), "input:r".into()],
        Monitoring {
            direct: false,
            software: true,
            doubled_path_confirmed: false,
        },
        &InsertRack::default(),
        &ProjectAuxRouting::default(),
    )
}

fn process_block(
    callback: &mut CallbackData,
    frames: usize,
    inputs: [&[f32]; 8],
    output_left: &mut [f32],
    output_right: &mut [f32],
) -> ProcessStatus {
    let publish = callback.armed.load(Ordering::Acquire);
    if frames > callback.plan.maximum_frames()
        || inputs.iter().any(|input| input.len() < frames)
        || output_left.len() < frames
        || output_right.len() < frames
    {
        callback.final_capture.callback_violation();
        output_left.fill(0.0);
        output_right.fill(0.0);
        return ProcessStatus::OversizedBlock;
    }
    for (node, source_kind, left, right) in [
        (SOURCE_NODE, BusSource::Synth, 0, 1),
        (LOOP_SOURCE_NODE, BusSource::Loop, 2, 3),
        (INPUT_SOURCE_NODE, BusSource::Input, 4, 5),
        (DRUM_SOURCE_NODE, BusSource::Drums, 6, 7),
    ] {
        let Some(source) = callback.plan.source_buffer_mut(node, frames) else {
            callback.final_capture.callback_violation();
            output_left[..frames].fill(0.0);
            output_right[..frames].fill(0.0);
            return ProcessStatus::OversizedBlock;
        };
        for ((frame, &left_sample), &right_sample) in source
            .iter_mut()
            .zip(inputs[left].iter())
            .zip(inputs[right].iter())
            .take(frames)
        {
            *frame = StereoFrame::new(left_sample, right_sample);
        }
        callback.final_bus.process_source(source_kind, source);
    }
    let status = callback.plan.process(frames);
    if !publish || !matches!(status, ProcessStatus::Complete) {
        if publish && !matches!(status, ProcessStatus::Complete) {
            callback.final_capture.callback_violation();
        }
        output_left[..frames].fill(0.0);
        output_right[..frames].fill(0.0);
        return status;
    }
    let Some(output) = callback.plan.output_buffer(SINK_NODE, frames) else {
        callback.final_capture.callback_violation();
        output_left[..frames].fill(0.0);
        output_right[..frames].fill(0.0);
        return ProcessStatus::OversizedBlock;
    };
    callback.final_buffer[..frames].copy_from_slice(output);
    callback
        .final_bus
        .process_final(&mut callback.final_buffer[..frames]);
    callback
        .final_capture
        .capture(&callback.final_buffer[..frames]);
    for index in 0..frames {
        output_left[index] = callback.final_buffer[index].left;
        output_right[index] = callback.final_buffer[index].right;
    }
    status
}

unsafe extern "C" fn process_callback(frames: c_uint, argument: *mut c_void) -> c_int {
    if argument.is_null() {
        return 0;
    }
    // SAFETY: OwnedAudioGraph pins CallbackData until JACK is inactive.
    let callback = unsafe { &mut *argument.cast::<CallbackData>() };
    let start = monotonic_nanoseconds();
    let get_buffer = callback.port_get_buffer;
    let mut input_pointers = [std::ptr::null_mut(); 8];
    for (pointer, port) in input_pointers.iter_mut().zip(callback.inputs) {
        *pointer = unsafe { get_buffer(port, frames) }.cast::<f32>();
    }
    let output_left = unsafe { get_buffer(callback.output_left, frames) }.cast::<f32>();
    let output_right = unsafe { get_buffer(callback.output_right, frames) }.cast::<f32>();
    if input_pointers.iter().any(|pointer| pointer.is_null())
        || output_left.is_null()
        || output_right.is_null()
    {
        callback.final_capture.invalid_buffer();
        return 0;
    }
    let frame_count = frames as usize;
    // SAFETY: JACK provides exactly `frames` f32 samples for each audio port.
    let inputs =
        input_pointers.map(|pointer| unsafe { std::slice::from_raw_parts(pointer, frame_count) });
    let output_left = unsafe { std::slice::from_raw_parts_mut(output_left, frame_count) };
    let output_right = unsafe { std::slice::from_raw_parts_mut(output_right, frame_count) };
    let status = process_block(callback, frame_count, inputs, output_left, output_right);
    let end = monotonic_nanoseconds();
    let elapsed = if start == 0 || end == 0 {
        0
    } else {
        end.saturating_sub(start)
    };
    callback
        .timing
        .record(frames, callback.sample_rate, elapsed, status);
    0
}

unsafe extern "C" fn shutdown_callback(argument: *mut c_void) {
    if !argument.is_null() {
        // SAFETY: OwnedAudioGraph pins CallbackData until client close.
        let callback = unsafe { &*argument.cast::<CallbackData>() };
        callback.client_lost.store(true, Ordering::Release);
        callback.final_capture.jack_shutdown();
    }
}

unsafe extern "C" fn xrun_callback(argument: *mut c_void) -> c_int {
    if !argument.is_null() {
        unsafe { &*argument.cast::<CallbackData>() }
            .final_capture
            .xrun();
    }
    0
}

unsafe extern "C" fn port_connect_callback(
    first: c_uint,
    second: c_uint,
    connected: c_int,
    argument: *mut c_void,
) {
    if connected != 0 || argument.is_null() {
        return;
    }
    let callback = unsafe { &*argument.cast::<CallbackData>() };
    if callback.armed.load(Ordering::Acquire)
        && callback.input_monitoring.load(Ordering::Acquire)
        && callback.input_port_ids[4..=5]
            .iter()
            .any(|port| *port == first || *port == second)
    {
        callback.source_lost.store(true, Ordering::Release);
        callback.final_capture.source_lost();
    }
}

fn monotonic_nanoseconds() -> u64 {
    let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock_gettime initializes the timespec on success.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, time.as_mut_ptr()) } != 0 {
        return 0;
    }
    // SAFETY: the successful call above initialized `time`.
    let time = unsafe { time.assume_init() };
    (time.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    #[derive(Default)]
    struct MockConnections {
        connected: BTreeSet<(String, String)>,
        operations: usize,
        fail_at: Option<usize>,
    }

    impl BoundaryConnections for MockConnections {
        fn connect(&mut self, connection: &Connection) -> Result<bool> {
            self.change(connection, true)
        }

        fn disconnect(&mut self, connection: &Connection) -> Result<bool> {
            self.change(connection, false)
        }
    }

    impl MockConnections {
        fn change(&mut self, connection: &Connection, connect: bool) -> Result<bool> {
            self.operations += 1;
            if self.fail_at == Some(self.operations) {
                bail!("injected connection failure");
            }
            let pair = (connection.source.clone(), connection.destination.clone());
            Ok(if connect {
                self.connected.insert(pair)
            } else {
                self.connected.remove(&pair)
            })
        }
    }

    fn routes() -> BoundaryRoutes {
        let graph_inputs = [
            "graph:in_l",
            "graph:in_r",
            "graph:loop_l",
            "graph:loop_r",
            "graph:input_l",
            "graph:input_r",
            "graph:drums_l",
            "graph:drums_r",
        ]
        .map(str::to_owned);
        BoundaryRoutes {
            required_graph: vec![
                connection("capture:l", "graph:input_l"),
                connection("capture:r", "graph:input_r"),
                connection("graph:out_l", "main:l"),
                connection("graph:out_r", "main:r"),
            ],
            optional_sources: vec![
                optional_source_routes(
                    BusSource::Synth,
                    ["engine:l".into(), "engine:r".into()],
                    &["main:l".into(), "main:r".into()],
                    ["graph:in_l", "graph:in_r"],
                ),
                optional_source_routes(
                    BusSource::Loop,
                    ["loop:l".into(), "loop:r".into()],
                    &["loop-playback:l".into(), "loop-playback:r".into()],
                    ["graph:loop_l", "graph:loop_r"],
                ),
            ],
            destinations: ["main:l".into(), "main:r".into()],
            loop_destinations: ["loop-playback:l".into(), "loop-playback:r".into()],
            graph_inputs,
            live_source_ports: ["capture:l".into(), "capture:r".into()],
        }
    }

    fn test_monitoring() -> Monitoring {
        Monitoring {
            direct: false,
            software: true,
            doubled_path_confirmed: false,
        }
    }

    fn test_live_ports() -> [String; 2] {
        ["capture:l".into(), "capture:r".into()]
    }

    fn callback(maximum_frames: u32) -> CallbackData {
        callback_with_recorder(maximum_frames, std::env::temp_dir(), true).0
    }

    fn callback_with_recorder(
        maximum_frames: u32,
        directory: PathBuf,
        input_monitoring: bool,
    ) -> (CallbackData, FinalMixRecorder, std::sync::Arc<BusControls>) {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let controls = std::sync::Arc::new(BusControls::default());
        for source in BusSource::ALL {
            assert!(controls.set_source_gain_db(source, 0.0));
        }
        controls.set_source_muted(BusSource::Input, !input_monitoring);
        let meters = std::sync::Arc::new(FinalBusMeters::default());
        let strip_controls = std::sync::Arc::new(
            MasterStripControls::new(48_000, &MasterStripSettings::default()).unwrap(),
        );
        let recorder =
            FinalMixRecorder::new(directory, 48_000, 4096, maximum_frames as usize).unwrap();
        let callback = CallbackData {
            plan: GraphPlan::compile(&dry_graph_definition(48_000, maximum_frames, &destinations))
                .unwrap(),
            inputs: [std::ptr::null_mut(); 8],
            input_port_ids: [10, 11, 12, 13, 14, 15, 16, 17],
            output_left: std::ptr::null_mut(),
            output_right: std::ptr::null_mut(),
            port_get_buffer: dummy_get_buffer,
            sample_rate: 48_000,
            armed: AtomicBool::new(false),
            client_lost: AtomicBool::new(false),
            source_lost: AtomicBool::new(false),
            input_monitoring: AtomicBool::new(input_monitoring),
            timing: CallbackTimingCounters::default(),
            final_bus: FinalBusProcessor::new(
                48_000,
                maximum_frames as usize,
                std::sync::Arc::clone(&controls),
                strip_controls,
                meters,
            )
            .unwrap(),
            final_capture: recorder.capture_handle(),
            final_buffer: vec![StereoFrame::SILENCE; maximum_frames as usize].into_boxed_slice(),
        };
        (callback, recorder, controls)
    }

    unsafe extern "C" fn dummy_get_buffer(_: *mut JackPort, _: c_uint) -> *mut c_void {
        std::ptr::null_mut()
    }

    #[test]
    fn dry_topology_is_valid_and_contains_exactly_four_sources() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let graph = dry_graph_definition(48_000, 128, &destinations);
        assert_eq!(
            graph.validate().unwrap(),
            [
                SOURCE_NODE,
                LOOP_SOURCE_NODE,
                INPUT_SOURCE_NODE,
                DRUM_SOURCE_NODE,
                MASTER_NODE,
                SINK_NODE
            ]
        );
        assert_eq!(graph.nodes.len(), 6);
        assert_eq!(graph.edges.len(), 5);
        assert!(graph.effects.is_empty());
    }

    #[test]
    fn graph_sums_four_distinguishable_stereo_sources_exactly_once() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let graph = dry_graph_definition(48_000, 64, &destinations);
        let mut plan = GraphPlan::compile(&graph).unwrap();
        for (node, left, right) in [
            (SOURCE_NODE, 0.01, 0.02),
            (LOOP_SOURCE_NODE, 0.04, 0.08),
            (INPUT_SOURCE_NODE, 0.16, 0.32),
            (DRUM_SOURCE_NODE, 0.03, 0.06),
        ] {
            plan.source_buffer_mut(node, 64)
                .unwrap()
                .fill(StereoFrame::new(left, right));
        }
        assert_eq!(plan.process(64), ProcessStatus::Complete);
        assert!(plan
            .output_buffer(SINK_NODE, 64)
            .unwrap()
            .iter()
            .all(|frame| {
                (frame.left - 0.24).abs() < 1e-7 && (frame.right - 0.48).abs() < 1e-7
            }));
    }

    #[test]
    fn managed_rack_builds_one_ordered_source_path() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let mut rack = InsertRack::default();
        let compressor = rack
            .add(crate::audio_graph::EffectKind::Compressor)
            .unwrap();
        let eq = rack.add(crate::audio_graph::EffectKind::Eq).unwrap();
        rack.move_to(eq, 0).unwrap();
        let graph = managed_graph_definition(
            48_000,
            128,
            &destinations,
            &test_live_ports(),
            test_monitoring(),
            &rack,
            &ProjectAuxRouting::default(),
        );
        assert_eq!(
            graph.validate().unwrap(),
            [
                SOURCE_NODE,
                LOOP_SOURCE_NODE,
                INPUT_SOURCE_NODE,
                DRUM_SOURCE_NODE,
                FIRST_EFFECT_NODE,
                FIRST_EFFECT_NODE + 1,
                MASTER_NODE,
                SINK_NODE
            ]
        );
        assert_eq!(graph.source_chains[0].effects, [eq, compressor]);
        assert_eq!(graph.edges.len(), 7);
    }

    #[test]
    fn master_chain_follows_level_change_and_empty_master_identity() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        for (routing, expected) in [
            (ProjectAuxRouting::default(), 0.5_f32),
            (
                {
                    let mut routing = ProjectAuxRouting::default();
                    routing
                        .master_rack
                        .add_with_id(crate::audio_graph::EffectKind::Utility, 1)
                        .unwrap();
                    routing
                        .master_rack
                        .effect_mut(1)
                        .unwrap()
                        .parameters
                        .insert("trim_db".into(), -6.0206);
                    routing
                },
                0.25_f32,
            ),
        ] {
            let graph = managed_graph_definition(
                48_000,
                64,
                &destinations,
                &test_live_ports(),
                test_monitoring(),
                &InsertRack::default(),
                &routing,
            );
            let mut plan = GraphPlan::compile(&graph).unwrap();
            plan.source_buffer_mut(SOURCE_NODE, 64)
                .unwrap()
                .fill(StereoFrame::new(0.5, -0.5));
            assert_eq!(plan.process(64), ProcessStatus::Complete);
            let output = plan.output_buffer(SINK_NODE, 64).unwrap();
            assert!(output.iter().all(|frame| {
                (frame.left - expected).abs() < 0.0001 && (frame.right + expected).abs() < 0.0001
            }));
        }
    }

    #[test]
    fn managed_aux_builds_one_scaled_pre_or_post_send_and_one_wet_return() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let mut rack = InsertRack::default();
        rack.add(crate::audio_graph::EffectKind::Eq).unwrap();
        let mut routing = ProjectAuxRouting::default();
        let aux = routing.add_bus().unwrap();
        let reverb = routing
            .add_effect(&rack, aux, crate::audio_graph::EffectKind::Reverb)
            .unwrap();
        let master = routing.next_effect_id(&rack).unwrap();
        routing
            .master_rack
            .add_with_id(crate::audio_graph::EffectKind::Compressor, master)
            .unwrap();
        routing
            .set_send(&rack, aux, -18.0, crate::audio_graph::SendPoint::PostInsert)
            .unwrap();
        let graph = managed_graph_definition(
            48_000,
            128,
            &destinations,
            &test_live_ports(),
            test_monitoring(),
            &rack,
            &routing,
        );
        graph.validate().unwrap();
        assert_eq!(graph.aux_buses[0].effects, [reverb]);
        assert_eq!(graph.master_chain, [master]);
        assert_eq!(graph.sends[0].level_db, -18.0);
        assert_eq!(
            graph
                .nodes
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::AuxReturn { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn activation_connection_failure_restores_the_exact_direct_topology() {
        let routes = routes();
        let mut connections = MockConnections::default();
        apply_transaction(&mut connections, &routes.direct_connection_changes()).unwrap();
        let direct = connections.connected.clone();
        connections.fail_at = Some(connections.operations + 3);
        assert!(apply_transaction(
            &mut connections,
            &BoundaryRoutes::source_connection_changes(&routes.optional_sources[0])
        )
        .is_err());
        assert_eq!(connections.connected, direct);
    }

    #[test]
    fn committed_activation_has_one_graph_path_and_no_direct_doubling() {
        let routes = routes();
        let mut connections = MockConnections::default();
        connections
            .connected
            .insert(("unrelated:out".into(), "unrelated:in".into()));
        apply_transaction(&mut connections, &routes.direct_connection_changes()).unwrap();
        apply_transaction(&mut connections, &routes.required_connection_changes()).unwrap();
        for source in &routes.optional_sources {
            apply_transaction(
                &mut connections,
                &BoundaryRoutes::source_connection_changes(source),
            )
            .unwrap();
        }
        let expected = routes
            .required_graph
            .iter()
            .map(|route| (route.source.clone(), route.destination.clone()))
            .chain(routes.optional_sources.iter().flat_map(|source| {
                source
                    .graph
                    .iter()
                    .map(|route| (route.source.clone(), route.destination.clone()))
            }))
            .chain(std::iter::once((
                "unrelated:out".into(),
                "unrelated:in".into(),
            )))
            .collect();
        assert_eq!(connections.connected, expected);
    }

    #[test]
    fn owner_shutdown_restores_only_owned_optional_routes() {
        let routes = routes();
        let unrelated = ("unrelated:out".to_owned(), "unrelated:in".to_owned());
        let mut connections = MockConnections::default();
        connections.connected.insert(unrelated.clone());
        apply_transaction(&mut connections, &routes.required_connection_changes()).unwrap();
        for source in &routes.optional_sources {
            apply_transaction(
                &mut connections,
                &BoundaryRoutes::source_connection_changes(source),
            )
            .unwrap();
        }

        // Closing the owned JACK client releases only graph-port boundaries.
        connections.connected.retain(|(source, destination)| {
            !source.starts_with("graph:") && !destination.starts_with("graph:")
        });
        apply_transaction(&mut connections, &routes.direct_connection_changes()).unwrap();

        let expected = routes
            .optional_sources
            .iter()
            .flat_map(|source| source.direct.iter())
            .map(|route| (route.source.clone(), route.destination.clone()))
            .chain(std::iter::once(unrelated))
            .collect();
        assert_eq!(connections.connected, expected);
    }

    #[test]
    fn input_only_activation_requires_only_exact_input_and_output_routes() {
        let routes = routes();
        let mut connections = MockConnections::default();
        apply_transaction(&mut connections, &routes.required_connection_changes()).unwrap();
        assert_eq!(
            connections.connected,
            routes
                .required_graph
                .iter()
                .map(|route| (route.source.clone(), route.destination.clone()))
                .collect()
        );
        assert!(!connections
            .connected
            .iter()
            .any(|(source, _)| source.starts_with("engine:") || source.starts_with("loop:")));
    }

    #[test]
    fn optional_sources_can_arrive_disappear_and_reconnect_without_duplicates() {
        let routes = routes();
        let source = &routes.optional_sources[0];
        let mut connections = MockConnections::default();
        apply_transaction(&mut connections, &routes.direct_connection_changes()).unwrap();
        let without_source = vec!["capture:l".into(), "capture:r".into()];
        assert!(!sync_optional_if_available(&mut connections, source, &without_source).unwrap());
        let direct = connections.connected.clone();

        let present = vec![source.ports[0].clone(), source.ports[1].clone()];
        assert!(sync_optional_if_available(&mut connections, source, &present).unwrap());
        assert!(source.graph.iter().all(|route| connections
            .connected
            .contains(&(route.source.clone(), route.destination.clone()))));
        assert!(source.direct.iter().all(|route| !connections
            .connected
            .contains(&(route.source.clone(), route.destination.clone()))));

        for route in &source.graph {
            connections
                .connected
                .remove(&(route.source.clone(), route.destination.clone()));
        }
        assert!(!sync_optional_if_available(&mut connections, source, &without_source).unwrap());
        assert!(sync_optional_if_available(&mut connections, source, &present).unwrap());
        let after_reconnect = connections.connected.clone();
        assert!(sync_optional_if_available(&mut connections, source, &present).unwrap());
        assert_eq!(connections.connected, after_reconnect);
        assert_ne!(connections.connected, direct);
    }

    #[test]
    fn input_monitor_controls_default_off_and_only_deliberate_enable_unmutes() {
        let controls = initial_bus_controls(false);
        assert!(controls.source_muted(BusSource::Input));
        assert!(!controls.source_muted(BusSource::Synth));
        let confirmed = initial_bus_controls(true);
        assert!(!confirmed.source_muted(BusSource::Input));
    }

    #[test]
    fn input_monitor_gate_and_final_wav_are_identical_to_post_master_playback() {
        let directory =
            std::env::temp_dir().join(format!("shr-input-final-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let (mut callback, mut recorder, controls) =
            callback_with_recorder(256, directory.clone(), false);
        callback.armed.store(true, Ordering::Release);
        recorder.start(Some("input-monitor")).unwrap();

        let silence = [0.0; 256];
        let input_left = [0.125; 256];
        let input_right = [-0.25; 256];
        let mut playback_left = [1.0; 256];
        let mut playback_right = [1.0; 256];
        let inputs = [
            &silence[..],
            &silence[..],
            &silence[..],
            &silence[..],
            &input_left[..],
            &input_right[..],
            &silence[..],
            &silence[..],
        ];
        assert_eq!(
            process_block(
                &mut callback,
                256,
                inputs,
                &mut playback_left,
                &mut playback_right,
            ),
            ProcessStatus::Complete
        );
        assert_eq!(playback_left, [0.0; 256]);
        assert_eq!(playback_right, [0.0; 256]);

        callback.input_monitoring.store(true, Ordering::Release);
        controls.set_source_muted(BusSource::Input, false);
        let mut monitored_left = [0.0; 256];
        let mut monitored_right = [0.0; 256];
        assert_eq!(
            process_block(
                &mut callback,
                256,
                inputs,
                &mut monitored_left,
                &mut monitored_right,
            ),
            ProcessStatus::Complete
        );
        assert!(monitored_left.iter().any(|sample| sample.abs() > 0.0));
        assert!(monitored_right.iter().any(|sample| sample.abs() > 0.0));

        recorder.request_stop();
        callback.final_capture.capture(&[]);
        recorder.finish_stop().unwrap();
        let path = recorder.status().path.unwrap();
        let bytes = std::fs::read(path).unwrap();
        let decoded = bytes[44..]
            .chunks_exact(6)
            .map(|frame| {
                let decode = |sample: &[u8]| {
                    let raw = i32::from(sample[0])
                        | (i32::from(sample[1]) << 8)
                        | (i32::from(sample[2]) << 16);
                    let signed = if raw & 0x80_0000 != 0 {
                        raw | !0xff_ffff
                    } else {
                        raw
                    };
                    signed as f32 / 8_388_607.0
                };
                StereoFrame::new(decode(&frame[..3]), decode(&frame[3..]))
            })
            .collect::<Vec<_>>();
        let playback = playback_left
            .iter()
            .zip(&playback_right)
            .chain(monitored_left.iter().zip(&monitored_right));
        for (recorded, (left, right)) in decoded.iter().zip(playback) {
            assert!((recorded.left - left).abs() <= 1.0 / 8_388_607.0);
            assert!((recorded.right - right).abs() <= 1.0 / 8_388_607.0);
        }
        assert_eq!(decoded.len(), 512);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn publication_is_block_boundary_dry_and_allocation_free() {
        let mut callback = callback(128);
        let left = [0.25; 128];
        let right = [-0.5; 128];
        let silence = [0.0; 128];
        let mut output_left = [1.0; 128];
        let mut output_right = [1.0; 128];
        assert_no_allocations(|| {
            assert_eq!(
                process_block(
                    &mut callback,
                    128,
                    [&left, &right, &silence, &silence, &silence, &silence, &silence, &silence,],
                    &mut output_left,
                    &mut output_right,
                ),
                ProcessStatus::Complete
            );
        });
        assert_eq!(output_left, [0.0; 128]);
        assert_eq!(output_right, [0.0; 128]);

        callback.armed.store(true, Ordering::Release);
        assert_no_allocations(|| {
            assert_eq!(
                process_block(
                    &mut callback,
                    128,
                    [&left, &right, &silence, &silence, &silence, &silence, &silence, &silence,],
                    &mut output_left,
                    &mut output_right,
                ),
                ProcessStatus::Complete
            );
        });
        assert_eq!(output_left, [0.0; 128]);
        assert_eq!(output_right, [0.0; 128]);

        assert_no_allocations(|| {
            assert_eq!(
                process_block(
                    &mut callback,
                    128,
                    [
                        &silence, &silence, &silence, &silence, &silence, &silence, &silence,
                        &silence,
                    ],
                    &mut output_left,
                    &mut output_right,
                ),
                ProcessStatus::Complete
            );
        });
        assert_eq!(&output_left[..5], &[0.0; 5]);
        assert_eq!(&output_left[5..], &[0.25; 123]);
        assert_eq!(&output_right[5..], &[-0.5; 123]);
    }

    #[test]
    fn oversized_callback_is_silent_and_countable_without_allocation() {
        let mut callback = callback(64);
        let input = [1.0; 128];
        let mut left = [1.0; 128];
        let mut right = [1.0; 128];
        assert_no_allocations(|| {
            let status = process_block(
                &mut callback,
                128,
                [
                    &input, &input, &input, &input, &input, &input, &input, &input,
                ],
                &mut left,
                &mut right,
            );
            callback.timing.record(128, 48_000, 10, status);
        });
        assert_eq!(left, [0.0; 128]);
        assert_eq!(right, [0.0; 128]);
        assert_eq!(callback.timing.snapshot().oversized_callbacks, 1);
    }

    #[test]
    fn callback_clock_reads_are_allocation_free() {
        assert_no_allocations(|| {
            let start = monotonic_nanoseconds();
            let end = monotonic_nanoseconds();
            assert!(end >= start);
        });
    }

    #[test]
    fn jack_shutdown_only_marks_client_loss_for_owner_recovery() {
        let mut callback = callback(64);
        assert!(!callback.client_lost.load(Ordering::Acquire));
        let pointer = ((&mut callback) as *mut CallbackData).cast();
        unsafe { shutdown_callback(pointer) };
        assert!(callback.client_lost.load(Ordering::Acquire));
    }

    #[test]
    fn optional_loss_is_silent_and_input_loss_faults_only_while_monitored() {
        let mut callback = callback(64);
        let pointer = ((&mut callback) as *mut CallbackData).cast();
        callback.armed.store(true, Ordering::Release);
        unsafe { port_connect_callback(10, 99, 0, pointer) };
        assert!(!callback.source_lost.load(Ordering::Acquire));

        callback.input_monitoring.store(false, Ordering::Release);
        unsafe { port_connect_callback(14, 99, 0, pointer) };
        assert!(!callback.source_lost.load(Ordering::Acquire));

        callback.input_monitoring.store(true, Ordering::Release);
        unsafe { port_connect_callback(15, 99, 0, pointer) };
        assert!(callback.source_lost.load(Ordering::Acquire));
    }

    #[test]
    fn ambiguous_boundaries_are_rejected_before_jack_activation() {
        let duplicate = ["same:port".to_owned(), "same:port".to_owned()];
        assert!(validate_stereo_boundary(&duplicate, "test").is_err());
        let empty = [String::new(), "right:port".to_owned()];
        assert!(validate_stereo_boundary(&empty, "test").is_err());
    }
}
