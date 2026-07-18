//! Disabled-by-default owner for the Phase 1 stereo JACK dry graph.
//!
//! All graph construction and JACK connection changes happen on the owner
//! thread. The callback only copies fixed buffers, runs a preallocated plan,
//! reads atomics, and updates lock-free counters.

use crate::audio_graph::{
    ChannelLayout, Edge, EffectInstance, EffectKind, GraphDefinition, Monitoring, Node, NodeKind,
    RecordingTap, SinkKind, SourceChain, SourceKind, StereoPorts, EFFECT_FORMAT_VERSION,
    GRAPH_FORMAT_VERSION,
};
use crate::audio_graph_runtime::{
    CallbackTimingCounters, CallbackTimingSnapshot, GraphPlan, ProcessStatus,
};
use crate::config::AudioGraphConfig;
use crate::dsp::StereoFrame;
use crate::jack::{Client as JackClient, Port as JackPort, PortDirection, PortGetBuffer};
use anyhow::{anyhow, bail, Context, Result};
use libc::{c_int, c_uint, c_void};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

const SOURCE_NODE: u32 = 1;
const UTILITY_NODE: u32 = 2;
const SINK_NODE: u32 = 3;
const UTILITY_EFFECT: u32 = 1;

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

struct BoundaryRoutes {
    direct: [Connection; 2],
    graph: [Connection; 4],
}

impl BoundaryRoutes {
    fn direct_connection_changes(&self) -> Vec<BoundaryChange> {
        self.direct
            .iter()
            .cloned()
            .map(|connection| BoundaryChange {
                kind: ChangeKind::Connect,
                connection,
            })
            .collect()
    }

    fn activation_changes(&self) -> Vec<BoundaryChange> {
        self.graph
            .iter()
            .cloned()
            .map(|connection| BoundaryChange {
                kind: ChangeKind::Connect,
                connection,
            })
            .chain(
                self.direct
                    .iter()
                    .cloned()
                    .map(|connection| BoundaryChange {
                        kind: ChangeKind::Disconnect,
                        connection,
                    }),
            )
            .collect()
    }
}

struct CallbackData {
    plan: GraphPlan,
    input_left: *mut JackPort,
    input_right: *mut JackPort,
    output_left: *mut JackPort,
    output_right: *mut JackPort,
    port_get_buffer: PortGetBuffer,
    sample_rate: u32,
    armed: AtomicBool,
    client_lost: AtomicBool,
    timing: CallbackTimingCounters,
}

// JACK owns callback scheduling, while the box itself remains pinned and is
// reclaimed only after deactivation on the non-real-time owner thread.
unsafe impl Send for CallbackData {}

pub(crate) struct OwnedAudioGraph {
    jack: JackClient,
    callback: Box<CallbackData>,
    routes: BoundaryRoutes,
}

impl OwnedAudioGraph {
    pub(crate) fn start(
        config: &AudioGraphConfig,
        source_ports: [String; 2],
        destinations: [String; 2],
    ) -> Result<Self> {
        validate_stereo_boundary(&source_ports, "managed-engine source")?;
        validate_stereo_boundary(&destinations, "main output")?;

        let mut jack = JackClient::open(&config.client_name).context("open owned audio graph")?;
        let sample_rate = jack.sample_rate();
        if sample_rate == 0 {
            bail!("JACK reported a zero sample rate");
        }
        let definition =
            dry_graph_definition(sample_rate, config.maximum_callback_frames, &destinations);
        let plan = GraphPlan::compile(&definition).context("compile dry audio graph")?;

        let input_left = jack.register_audio_port("managed_in_l", PortDirection::Input)?;
        let input_right = jack.register_audio_port("managed_in_r", PortDirection::Input)?;
        let output_left = jack.register_audio_port("main_out_l", PortDirection::Output)?;
        let output_right = jack.register_audio_port("main_out_r", PortDirection::Output)?;
        let graph_port_names = [
            jack.port_name_string(input_left)?,
            jack.port_name_string(input_right)?,
            jack.port_name_string(output_left)?,
            jack.port_name_string(output_right)?,
        ];
        let routes = BoundaryRoutes {
            direct: [
                connection(&source_ports[0], &destinations[0]),
                connection(&source_ports[1], &destinations[1]),
            ],
            graph: [
                connection(&source_ports[0], &graph_port_names[0]),
                connection(&source_ports[1], &graph_port_names[1]),
                connection(&graph_port_names[2], &destinations[0]),
                connection(&graph_port_names[3], &destinations[1]),
            ],
        };
        let mut callback = Box::new(CallbackData {
            plan,
            input_left,
            input_right,
            output_left,
            output_right,
            port_get_buffer: jack.port_get_buffer(),
            sample_rate,
            armed: AtomicBool::new(false),
            client_lost: AtomicBool::new(false),
            timing: CallbackTimingCounters::default(),
        });
        let callback_pointer = ((&mut *callback) as *mut CallbackData).cast();
        // SAFETY: callback remains boxed until after explicit JACK deactivation.
        unsafe {
            jack.set_process_callback(process_callback, callback_pointer)?;
            jack.set_shutdown_callback(shutdown_callback, callback_pointer);
        }
        jack.activate().context("activate owned audio graph")?;
        // Re-establish the conservative route through JACK's checked API even
        // if the legacy jack_connect helper was unavailable or raced startup.
        if let Err(error) = apply_transaction(&mut jack, &routes.direct_connection_changes()) {
            jack.deactivate();
            return Err(error.context("establish direct fallback before graph routing"));
        }
        if let Err(error) = apply_transaction(&mut jack, &routes.activation_changes()) {
            jack.deactivate();
            return Err(error.context("activate owned graph boundary"));
        }
        // The callback samples this once per block. All graph connections are
        // ready and both direct links are gone before dry output is published.
        callback.armed.store(true, Ordering::Release);
        Ok(Self {
            jack,
            callback,
            routes,
        })
    }

    pub(crate) fn client_lost(&self) -> bool {
        self.callback.client_lost.load(Ordering::Acquire)
    }

    pub(crate) fn timing(&self) -> CallbackTimingSnapshot {
        self.callback.timing.snapshot()
    }

    /// Restore both exact direct links best-effort. This runs only on the
    /// non-real-time owner thread, including client-loss recovery.
    pub(crate) fn restore_direct(&mut self) -> Result<()> {
        self.callback.armed.store(false, Ordering::Release);
        // Join the callback before creating either direct link. A callback
        // that sampled the previous publish flag can therefore never overlap
        // the restored dry path for even one block.
        self.jack.deactivate();
        let mut first_error = None;
        for connection in &self.routes.direct {
            if let Err(error) = self
                .jack
                .ensure_connection(&connection.source, &connection.destination)
            {
                first_error.get_or_insert(error);
            }
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

fn validate_stereo_boundary(ports: &[String; 2], description: &str) -> Result<()> {
    if ports.iter().any(|port| port.trim().is_empty()) {
        bail!("{description} contains an empty JACK port name");
    }
    if ports[0] == ports[1] {
        bail!("{description} JACK ports are ambiguous");
    }
    Ok(())
}

fn dry_graph_definition(
    sample_rate: u32,
    maximum_callback_frames: u32,
    destinations: &[String; 2],
) -> GraphDefinition {
    GraphDefinition {
        format_version: GRAPH_FORMAT_VERSION,
        enabled: true,
        sample_rate,
        maximum_callback_frames,
        nodes: vec![
            Node {
                id: SOURCE_NODE,
                layout: ChannelLayout::Stereo,
                kind: NodeKind::Source {
                    source: SourceKind::ManagedEngine,
                },
            },
            Node {
                id: UTILITY_NODE,
                layout: ChannelLayout::Stereo,
                kind: NodeKind::Processor {
                    effect_id: UTILITY_EFFECT,
                },
            },
            Node {
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
            },
        ],
        edges: vec![
            Edge {
                id: 1,
                from: SOURCE_NODE,
                to: UTILITY_NODE,
            },
            Edge {
                id: 2,
                from: UTILITY_NODE,
                to: SINK_NODE,
            },
        ],
        effects: vec![EffectInstance {
            id: UTILITY_EFFECT,
            kind: EffectKind::Utility,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: BTreeMap::new(),
            owned_memory_bytes: 0,
        }],
        source_chains: vec![SourceChain {
            source_node: SOURCE_NODE,
            effects: vec![UTILITY_EFFECT],
        }],
        master_chain: vec![],
        aux_buses: vec![],
        sends: vec![],
        monitoring: Monitoring::default(),
        recording_tap: RecordingTap::PostMaster,
    }
}

fn process_block(
    callback: &mut CallbackData,
    frames: usize,
    input_left: &[f32],
    input_right: &[f32],
    output_left: &mut [f32],
    output_right: &mut [f32],
) -> ProcessStatus {
    let publish = callback.armed.load(Ordering::Acquire);
    if frames > callback.plan.maximum_frames()
        || input_left.len() < frames
        || input_right.len() < frames
        || output_left.len() < frames
        || output_right.len() < frames
    {
        output_left.fill(0.0);
        output_right.fill(0.0);
        return ProcessStatus::OversizedBlock;
    }
    let Some(source) = callback.plan.source_buffer_mut(SOURCE_NODE, frames) else {
        output_left[..frames].fill(0.0);
        output_right[..frames].fill(0.0);
        return ProcessStatus::OversizedBlock;
    };
    for index in 0..frames {
        source[index] = StereoFrame::new(input_left[index], input_right[index]);
    }
    let status = callback.plan.process(frames);
    if !publish || !matches!(status, ProcessStatus::Complete) {
        output_left[..frames].fill(0.0);
        output_right[..frames].fill(0.0);
        return status;
    }
    let Some(output) = callback.plan.output_buffer(SINK_NODE, frames) else {
        output_left[..frames].fill(0.0);
        output_right[..frames].fill(0.0);
        return ProcessStatus::OversizedBlock;
    };
    for index in 0..frames {
        output_left[index] = output[index].left;
        output_right[index] = output[index].right;
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
    let input_left = unsafe { get_buffer(callback.input_left, frames) }.cast::<f32>();
    let input_right = unsafe { get_buffer(callback.input_right, frames) }.cast::<f32>();
    let output_left = unsafe { get_buffer(callback.output_left, frames) }.cast::<f32>();
    let output_right = unsafe { get_buffer(callback.output_right, frames) }.cast::<f32>();
    if input_left.is_null()
        || input_right.is_null()
        || output_left.is_null()
        || output_right.is_null()
    {
        return 0;
    }
    let frame_count = frames as usize;
    // SAFETY: JACK provides exactly `frames` f32 samples for each audio port.
    let input_left = unsafe { std::slice::from_raw_parts(input_left, frame_count) };
    let input_right = unsafe { std::slice::from_raw_parts(input_right, frame_count) };
    let output_left = unsafe { std::slice::from_raw_parts_mut(output_left, frame_count) };
    let output_right = unsafe { std::slice::from_raw_parts_mut(output_right, frame_count) };
    let status = process_block(
        callback,
        frame_count,
        input_left,
        input_right,
        output_left,
        output_right,
    );
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
        unsafe { &*argument.cast::<CallbackData>() }
            .client_lost
            .store(true, Ordering::Release);
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
        BoundaryRoutes {
            direct: [
                connection("engine:l", "main:l"),
                connection("engine:r", "main:r"),
            ],
            graph: [
                connection("engine:l", "graph:in_l"),
                connection("engine:r", "graph:in_r"),
                connection("graph:out_l", "main:l"),
                connection("graph:out_r", "main:r"),
            ],
        }
    }

    fn callback(maximum_frames: u32) -> CallbackData {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        CallbackData {
            plan: GraphPlan::compile(&dry_graph_definition(48_000, maximum_frames, &destinations))
                .unwrap(),
            input_left: std::ptr::null_mut(),
            input_right: std::ptr::null_mut(),
            output_left: std::ptr::null_mut(),
            output_right: std::ptr::null_mut(),
            port_get_buffer: dummy_get_buffer,
            sample_rate: 48_000,
            armed: AtomicBool::new(false),
            client_lost: AtomicBool::new(false),
            timing: CallbackTimingCounters::default(),
        }
    }

    unsafe extern "C" fn dummy_get_buffer(_: *mut JackPort, _: c_uint) -> *mut c_void {
        std::ptr::null_mut()
    }

    #[test]
    fn dry_topology_is_valid_and_contains_one_managed_path() {
        let destinations = ["main:l".to_owned(), "main:r".to_owned()];
        let graph = dry_graph_definition(48_000, 128, &destinations);
        assert_eq!(
            graph.validate().unwrap(),
            [SOURCE_NODE, UTILITY_NODE, SINK_NODE]
        );
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.effects.len(), 1);
    }

    #[test]
    fn activation_connection_failure_restores_the_exact_direct_topology() {
        let routes = routes();
        let mut connections = MockConnections::default();
        apply_transaction(&mut connections, &routes.direct_connection_changes()).unwrap();
        let direct = connections.connected.clone();
        connections.fail_at = Some(8);
        assert!(apply_transaction(&mut connections, &routes.activation_changes()).is_err());
        assert_eq!(connections.connected, direct);
    }

    #[test]
    fn committed_activation_has_one_graph_path_and_no_direct_doubling() {
        let routes = routes();
        let mut connections = MockConnections::default();
        connections
            .connected
            .insert(("unrelated:out".into(), "unrelated:in".into()));
        for direct in &routes.direct {
            connections
                .connected
                .insert((direct.source.clone(), direct.destination.clone()));
        }
        apply_transaction(&mut connections, &routes.activation_changes()).unwrap();
        let expected = routes
            .graph
            .iter()
            .map(|route| (route.source.clone(), route.destination.clone()))
            .chain(std::iter::once((
                "unrelated:out".into(),
                "unrelated:in".into(),
            )))
            .collect();
        assert_eq!(connections.connected, expected);
    }

    #[test]
    fn publication_is_block_boundary_dry_and_allocation_free() {
        let mut callback = callback(128);
        let left = [0.25; 128];
        let right = [-0.5; 128];
        let mut output_left = [1.0; 128];
        let mut output_right = [1.0; 128];
        assert_no_allocations(|| {
            assert_eq!(
                process_block(
                    &mut callback,
                    128,
                    &left,
                    &right,
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
                    &left,
                    &right,
                    &mut output_left,
                    &mut output_right,
                ),
                ProcessStatus::Complete
            );
        });
        assert_eq!(output_left, left);
        assert_eq!(output_right, right);
    }

    #[test]
    fn oversized_callback_is_silent_and_countable_without_allocation() {
        let mut callback = callback(64);
        let input = [1.0; 128];
        let mut left = [1.0; 128];
        let mut right = [1.0; 128];
        assert_no_allocations(|| {
            let status = process_block(&mut callback, 128, &input, &input, &mut left, &mut right);
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
    fn ambiguous_boundaries_are_rejected_before_jack_activation() {
        let duplicate = ["same:port".to_owned(), "same:port".to_owned()];
        assert!(validate_stereo_boundary(&duplicate, "test").is_err());
        let empty = [String::new(), "right:port".to_owned()];
        assert!(validate_stereo_boundary(&empty, "test").is_err());
    }
}
