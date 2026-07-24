pub mod audio_graph;
mod audio_graph_client;
pub mod audio_graph_runtime;
mod audio_recorder;
mod chord;
mod config;
mod control;
mod controller_learn;
mod controller_profile;
#[cfg(test)]
mod demo;
mod device_profile;
mod drum_pattern;
pub mod dsp;
pub mod effect_schema;
pub mod effects;
mod engine;
mod final_bus;
mod fsutil;
mod geometry;
mod gm;
mod help;
mod jack;
mod loop_player;
mod midi;
mod midi_endpoint;
mod navigation;
mod note_lifecycle;
mod overlay;
mod pads;
mod performance_meter;
mod preset;
mod recording;
mod scale;
mod sequencer;
mod startup_splash;
mod ui;
mod ui_text;

use anyhow::{bail, Context, Result};
use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    if let Err(e) = real_main() {
        eprintln!("shr: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let state = state_dir();
    let args: Vec<String> = env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("help" | "-h" | "--help")
    ) {
        usage();
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("config") {
        return config_command(&args[1..], &state);
    }
    if args.first().map(String::as_str) == Some("recorder-stress") {
        return recorder_stress_command(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("final-mix-stress") {
        return final_mix_stress_command(&args[1..]);
    }
    let runtime = config::RuntimeConfig::load(&state.join("shsynth.conf"))?;
    let preset_dir = preset_dir(&runtime)?;
    let catalogs = preset::discover_all(&runtime, &preset_dir);
    let presets = catalogs
        .iter()
        .flat_map(|catalog| catalog.presets.iter().cloned())
        .collect::<Vec<_>>();
    match args.first().map(String::as_str).unwrap_or("menu") {
        "menu" => ui::run(&catalogs, &state, &runtime),
        "list" => {
            for catalog in &catalogs {
                if let Some(reason) = &catalog.unavailable {
                    println!("[{} unavailable: {reason}]", catalog.backend.label());
                }
                for p in &catalog.presets {
                    println!("{}:{}", catalog.backend.label(), p.name);
                }
            }
            Ok(())
        }
        "status" => {
            println!("{}", engine::status(&state));
            print_midi_input_status(&runtime, &state);
            Ok(())
        }
        "doctor" => doctor(&runtime, &preset_dir, &state),
        "screenshots" => {
            println!("{}", ui::readme_screenshots_json(&runtime)?);
            Ok(())
        }
        "stop" => engine::stop_managed(&state),
        "log" | "logs" => show_log(&state, args.get(1)),
        "ideas" => ideas_command(&args[1..], &presets, &state, &runtime),
        "pads" => pads_command(&args[1..], &state),
        "clock" => clock_command(&args[1..], &runtime),
        "casio" => casio_command(&args[1..], &runtime),
        "effects-checkpoint" | "phase2-checkpoint" => {
            effects_checkpoint(&args[1..], &presets, &state, &runtime)
        }
        "start" => {
            let arg = args.get(1).context("usage: shr start PRESET")?;
            let p = preset::resolve(&presets, arg)
                .with_context(|| format!("unknown preset (use ENGINE:NAME): {arg}"))?;
            start_daemon(p, arg, &state, &runtime)
        }
        "daemon" => {
            let arg = args.get(1).context("internal daemon missing preset")?;
            let p = preset::resolve(&presets, arg)
                .with_context(|| format!("unknown preset: {arg}"))?
                .clone();
            engine::daemon(p, state, runtime)
        }
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            usage();
            bail!("unknown command: {other}")
        }
    }
}

fn effects_checkpoint(
    args: &[String],
    presets: &[preset::Preset],
    state: &Path,
    config: &config::RuntimeConfig,
) -> Result<()> {
    checkpoint_event("command-start");
    let selector = args
        .first()
        .context("usage: shr effects-checkpoint PRESET [PROFILE] [SECONDS]")?;
    let preset = preset::resolve(presets, selector)
        .with_context(|| format!("unknown preset (use ENGINE:NAME): {selector}"))?;
    let profile = args.get(1).map(String::as_str).unwrap_or("full");
    let seconds = args
        .get(2)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(10);
    if !(1..=60).contains(&seconds) {
        bail!("checkpoint duration must be 1..=60 seconds");
    }
    let (rack, aux_routing) = effects_routing(profile)?;
    let mut config = config.clone();
    config.audio_graph.enabled = true;
    // The production final bus has a fixed loop-source boundary. A standalone
    // checkpoint has no UI-owned loop client, so provide an owned silent source
    // only when that exact configured client is absent. Its direct links are
    // removed/restored by the normal graph transaction and disappear on drop.
    let checkpoint_loop = checkpoint_loop_source(&config)?;
    checkpoint_event("loop-source-ready");
    let maximum_frames = config.audio_graph.maximum_callback_frames as usize;
    let node_count = 3usize
        .saturating_add(rack.order.len())
        .saturating_add(aux_routing.master_rack.order.len())
        .saturating_add(
            aux_routing
                .buses
                .iter()
                .map(|bus| {
                    bus.rack.order.len()
                        + 1
                        + usize::from(aux_routing.sends.iter().any(|send| send.aux_id == bus.id))
                })
                .sum::<usize>(),
        );
    let graph_buffer_bytes = node_count
        .saturating_mul(maximum_frames)
        .saturating_mul(std::mem::size_of::<dsp::StereoFrame>());
    let (tx, _) = std::sync::mpsc::channel();
    let router = engine::MidiRouter::start(state, &config, tx)?;
    checkpoint_event("midi-router-ready");
    let mut engine = engine::Engine::start_with_routing(
        preset,
        state,
        router.output(),
        &config,
        &rack,
        &aux_routing,
    )?;
    engine.bind_midi_lifecycle(router.lifecycle());
    checkpoint_event("engine-and-graph-ready");
    let sample_rate = engine
        .audio_graph_sample_rate()
        .context("owned graph sample rate unavailable")?;
    let effect_memory_bytes = rack
        .effects
        .iter()
        .chain(aux_routing.master_rack.effects.iter())
        .chain(
            aux_routing
                .buses
                .iter()
                .flat_map(|bus| bus.rack.effects.iter()),
        )
        .map(|effect| {
            effect_schema::minimum_runtime_memory_bytes(effect.kind, sample_rate, maximum_frames)
        })
        .sum::<usize>();
    let owner_pid = std::process::id();
    let synth_pid = engine.process_id();
    let owner_start = process_ticks(owner_pid).unwrap_or(0);
    let synth_start = process_ticks(synth_pid).unwrap_or(0);
    let mut owner_rss_kib = process_rss_kib(owner_pid).unwrap_or(0);
    let mut synth_rss_kib = process_rss_kib(synth_pid).unwrap_or(0);
    let started = Instant::now();
    engine.send(&[0x90, 48, 8])?;
    checkpoint_event("note-on-sent");
    while started.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(Duration::from_millis(100));
        owner_rss_kib = owner_rss_kib.max(process_rss_kib(owner_pid).unwrap_or(0));
        synth_rss_kib = synth_rss_kib.max(process_rss_kib(synth_pid).unwrap_or(0));
    }
    let final_meter = engine
        .final_bus_meter()
        .context("owned graph final meter unavailable")?;
    checkpoint_event("measurement-complete");
    let _ = engine.send(&[0x80, 48, 0]);
    // `Engine::drop` sends the full all-channel panic immediately before it
    // terminates the owned synth. Do not send the same 48-message burst twice
    // during this tightly bounded checkpoint teardown.
    checkpoint_event("note-off-sent");
    let elapsed = started.elapsed().as_secs_f64();
    let owner_ticks = process_ticks(owner_pid)
        .unwrap_or(owner_start)
        .saturating_sub(owner_start);
    let synth_ticks = process_ticks(synth_pid)
        .unwrap_or(synth_start)
        .saturating_sub(synth_start);
    let clock_ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let cpu_percent = |ticks: u64| ticks as f64 / clock_ticks / elapsed * 100.0;
    checkpoint_event("graph-restore-start");
    let (timing, restored) = engine
        .finish_audio_graph_checkpoint()
        .context("owned graph fell back before checkpoint completed")?;
    checkpoint_event("graph-restored");
    println!(
        "PROFILE {profile} · effects {} · {elapsed:.3} s",
        rack.effects.len()
            + aux_routing.master_rack.effects.len()
            + aux_routing
                .buses
                .iter()
                .map(|bus| bus.rack.effects.len())
                .sum::<usize>()
    );
    println!(
        "AUDIO GRAPH METRICS · {}",
        engine::audio_graph_metrics(&timing)
    );
    println!(
        "OWNER CPU {:.2}% · max RSS {} KiB · SYNTH CPU {:.2}% · max RSS {} KiB",
        cpu_percent(owner_ticks),
        owner_rss_kib,
        cpu_percent(synth_ticks),
        synth_rss_kib
    );
    println!(
        "PREALLOCATED AUDIO ARRAYS · effects {} bytes · graph buffers {} bytes · capacity {} frames",
        effect_memory_bytes, graph_buffer_bytes, maximum_frames
    );
    println!(
        "FINAL BUS METERS · limiter input peak {:.6}/{:.6} rms {:.6}/{:.6} clips {} non-finite {} · output peak {:.6}/{:.6} rms {:.6}/{:.6} clips {} non-finite {} · limiter reduction {:.3} dB",
        final_meter.limiter_input.peak.left,
        final_meter.limiter_input.peak.right,
        final_meter.limiter_input.rms.left,
        final_meter.limiter_input.rms.right,
        final_meter.limiter_input.clips,
        final_meter.limiter_input.non_finite,
        final_meter.output.peak.left,
        final_meter.output.peak.right,
        final_meter.output.rms.left,
        final_meter.output.rms.right,
        final_meter.output.clips,
        final_meter.output.non_finite,
        final_meter.limiter_gain_reduction_db,
    );
    restored.context("restore exact direct route after checkpoint")?;
    checkpoint_event("engine-drop-start");
    drop(engine);
    checkpoint_event("engine-dropped");
    drop(router);
    checkpoint_event("midi-router-dropped");
    drop(checkpoint_loop);
    checkpoint_event("loop-source-dropped");
    Ok(())
}

fn checkpoint_event(label: &str) {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    println!(
        "CHECKPOINT EVENT · unix_us={} · {label}",
        elapsed.as_micros()
    );
}

fn checkpoint_loop_source(
    config: &config::RuntimeConfig,
) -> Result<Option<loop_player::LoopPlayer>> {
    let ports = loop_player::configured_output_ports(&config.loop_player);
    let available = engine::jack_ports();
    let present = ports.map(|port| available.iter().any(|candidate| candidate == &port));
    match present {
        [true, true] => return Ok(None),
        [true, false] | [false, true] => {
            bail!("configured checkpoint loop source has only one stereo port available")
        }
        [false, false] => {}
    }

    let probe_name = format!("{}-checkpoint-probe", config.audio_graph.client_name);
    let probe = jack::Client::open(&probe_name).context("read JACK rate for checkpoint source")?;
    let sample_rate = probe.sample_rate();
    drop(probe);
    if !(8_000..=384_000).contains(&sample_rate) {
        bail!("unsupported JACK rate for checkpoint source: {sample_rate}");
    }

    let clock = std::sync::Arc::new(loop_player::TransportClock::default());
    let mut player = loop_player::LoopPlayer::new(&config.loop_player, clock);
    player.load(
        loop_player::DecodedLoop {
            samples: vec![[0.0; 2]; sample_rate as usize],
            sample_rate,
            channels: 2,
        },
        &sequencer::LoopSettings {
            file: "effects-checkpoint-silence".into(),
            source_bpm_x100: 12_000,
            interpretation: sequencer::BpmInterpretation::Normal,
            start_beat: 0,
            length_beats: 1,
            offset_beats: 0,
        },
    )?;
    Ok(Some(player))
}

fn effects_routing(
    profile: &str,
) -> Result<(audio_graph::InsertRack, audio_graph::ProjectAuxRouting)> {
    use audio_graph::EffectKind;
    let mut rack = audio_graph::InsertRack::default();
    let mut aux_routing = audio_graph::ProjectAuxRouting::default();
    match profile {
        "dry" => {}
        "eq" => add_profile_effect(
            &mut rack,
            EffectKind::Eq,
            &[("low_cut_enabled", 1.0), ("low_cut_hz", 80.0)],
        )?,
        "compressor" => add_profile_effect(
            &mut rack,
            EffectKind::Compressor,
            &[("threshold_db", -24.0)],
        )?,
        "soft-cubic" => add_profile_effect(&mut rack, EffectKind::Distortion, &[("mode", 0.0)])?,
        "hard-clip" => add_profile_effect(&mut rack, EffectKind::Distortion, &[("mode", 1.0)])?,
        "asymmetric" => add_profile_effect(&mut rack, EffectKind::Distortion, &[("mode", 2.0)])?,
        "gate" => add_profile_effect(&mut rack, EffectKind::Gate, &[])?,
        "filter-lp" => add_profile_effect(&mut rack, EffectKind::Filter, &[("mode", 0.0)])?,
        "filter-bp" => add_profile_effect(&mut rack, EffectKind::Filter, &[("mode", 1.0)])?,
        "filter-hp" => add_profile_effect(&mut rack, EffectKind::Filter, &[("mode", 2.0)])?,
        "crusher" => add_profile_effect(
            &mut rack,
            EffectKind::Crusher,
            &[("bit_depth", 8.0), ("hold_factor", 4.0)],
        )?,
        "full" => {
            add_profile_effect(
                &mut rack,
                EffectKind::Eq,
                &[("low_cut_enabled", 1.0), ("low_cut_hz", 80.0)],
            )?;
            add_profile_effect(
                &mut rack,
                EffectKind::Compressor,
                &[("threshold_db", -24.0)],
            )?;
            add_profile_effect(&mut rack, EffectKind::Distortion, &[("mode", 2.0)])?;
            add_profile_effect(
                &mut rack,
                EffectKind::Crusher,
                &[("bit_depth", 10.0), ("hold_factor", 2.0)],
            )?;
            add_profile_effect(&mut rack, EffectKind::Gate, &[])?;
            add_profile_effect(&mut rack, EffectKind::Filter, &[("mode", 0.0)])?;
            add_profile_effect(&mut rack, EffectKind::Eq, &[])?;
            add_profile_effect(&mut rack, EffectKind::Compressor, &[])?;
        }
        "delay" => add_profile_effect(&mut rack, EffectKind::Delay, &[])?,
        "chorus" => add_profile_effect(&mut rack, EffectKind::Chorus, &[])?,
        "flanger" => add_profile_effect(&mut rack, EffectKind::Flanger, &[])?,
        "phaser" => add_profile_effect(&mut rack, EffectKind::Phaser, &[])?,
        "tremolo" => add_profile_effect(&mut rack, EffectKind::TremoloPan, &[("mode", 0.0)])?,
        "autopan" => add_profile_effect(&mut rack, EffectKind::TremoloPan, &[("mode", 1.0)])?,
        "time-full" => {
            for kind in [
                EffectKind::Delay,
                EffectKind::Chorus,
                EffectKind::Flanger,
                EffectKind::Phaser,
                EffectKind::TremoloPan,
                EffectKind::Eq,
                EffectKind::Compressor,
                EffectKind::Filter,
            ] {
                add_profile_effect(&mut rack, kind, &[])?;
            }
        }
        "reverb-room" | "reverb-plate" | "reverb-hall" | "two-reverbs" | "phase4-full" => {
            if profile == "phase4-full" {
                for kind in [
                    EffectKind::Eq,
                    EffectKind::Compressor,
                    EffectKind::Delay,
                    EffectKind::Chorus,
                    EffectKind::Flanger,
                    EffectKind::Phaser,
                    EffectKind::TremoloPan,
                    EffectKind::Filter,
                ] {
                    add_profile_effect(&mut rack, kind, &[])?;
                }
            }
            let first = aux_routing
                .add_bus()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let first_type = match profile {
                "reverb-plate" => 1.0,
                "reverb-hall" => 2.0,
                _ => 0.0,
            };
            let first_effect = aux_routing
                .add_effect(&rack, first, EffectKind::Reverb)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            aux_routing.buses[0]
                .rack
                .effect_mut(first_effect)
                .context("first reverb missing")?
                .parameters
                .insert("type".into(), first_type);
            aux_routing
                .set_send(&rack, first, -18.0, audio_graph::SendPoint::PostInsert)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if matches!(profile, "two-reverbs" | "phase4-full") {
                let second = aux_routing
                    .add_bus()
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let second_effect = aux_routing
                    .add_effect(&rack, second, EffectKind::Reverb)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                aux_routing.buses[1]
                    .rack
                    .effect_mut(second_effect)
                    .context("second reverb missing")?
                    .parameters
                    .insert("type".into(), 2.0);
                aux_routing
                    .set_send(&rack, second, -24.0, audio_graph::SendPoint::PreInsert)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            }
            if profile == "phase4-full" {
                let id = aux_routing
                    .next_effect_id(&rack)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                aux_routing
                    .master_rack
                    .add_with_id(EffectKind::Compressor, id)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            }
        }
        _ => bail!("unknown checkpoint profile {profile}"),
    }
    aux_routing
        .validate(&rack)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok((rack, aux_routing))
}

fn add_profile_effect(
    rack: &mut audio_graph::InsertRack,
    kind: audio_graph::EffectKind,
    parameters: &[(&str, f32)],
) -> Result<()> {
    let id = rack
        .add(kind)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let effect = rack.effect_mut(id).context("new rack effect missing")?;
    for (name, value) in parameters {
        effect.parameters.insert((*name).into(), *value);
    }
    Ok(())
}

fn process_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields = stat
        .get(stat.rfind(')')? + 2..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

fn process_rss_kib(pid: u32) -> Option<u64> {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn preset_dir(config: &config::RuntimeConfig) -> Result<PathBuf> {
    if let Some(path) = env::var_os("SHSYNTH_PRESET_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = &config.preset_dir {
        return Ok(path.clone());
    }
    let beside_exe = env::current_exe()?
        .parent()
        .unwrap_or(Path::new("."))
        .join("../share/shsynth/presets/synthv1");
    if beside_exe.is_dir() {
        return Ok(beside_exe);
    }
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1"))
}

fn state_dir() -> PathBuf {
    if let Some(path) = env::var_os("SHSYNTH_STATE_DIR") {
        return PathBuf::from(path);
    }
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".local/state")
        })
        .join("shsynth")
}

fn start_daemon(
    preset: &preset::Preset,
    selector: &str,
    state: &Path,
    config: &config::RuntimeConfig,
) -> Result<()> {
    engine::validate_start(preset, state, config)?;
    engine::stop_managed(state)?;
    fs::create_dir_all(state)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join("engine.log"))?;
    let exe = env::current_exe()?;
    let mut child = Command::new(exe)
        .args(daemon_args(selector))
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    let deadline = Instant::now() + config.startup_timeout + Duration::from_secs(1);
    let failure = loop {
        if Instant::now() >= deadline {
            break anyhow::anyhow!("engine startup timed out");
        }
        thread::sleep(Duration::from_millis(100));
        match child.try_wait() {
            Ok(Some(status)) => break anyhow::anyhow!("engine daemon exited with {status}"),
            Ok(None) => {}
            Err(error) => break anyhow::Error::new(error).context("check engine daemon status"),
        }
        let status = engine::status(state);
        if status.starts_with("Running:") {
            println!("Loaded {}:{}.", preset.backend.label(), preset.name);
            return Ok(());
        }
    };
    let cleanup = cleanup_failed_daemon(&mut child, state);
    let log_path = state.join("engine.log");
    match cleanup {
        Ok(()) => Err(failure.context(format!("see {}", log_path.display()))),
        Err(error) => Err(failure.context(format!(
            "see {}; cleanup also failed: {error:#}",
            log_path.display()
        ))),
    }
}

fn daemon_args(selector: &str) -> [&str; 2] {
    ["daemon", selector]
}

fn cleanup_failed_daemon(child: &mut Child, state: &Path) -> Result<()> {
    let termination = terminate_child(child);
    let managed = engine::stop_managed(state);
    termination.context("terminate failed engine daemon")?;
    managed.context("stop engine left by failed daemon startup")
}

fn terminate_child(child: &mut Child) -> std::io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(error),
    }
    child.wait().map(|_| ())
}

fn show_log(state: &Path, count: Option<&String>) -> Result<()> {
    let n = count.and_then(|s| s.parse::<usize>().ok()).unwrap_or(80);
    let text = fs::read_to_string(state.join("engine.log")).unwrap_or_default();
    let lines: Vec<_> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(n)..] {
        println!("{line}");
    }
    Ok(())
}

fn usage() {
    println!(
        "Usage: shr [COMMAND]\n\
         \n\
         With no command, opens the terminal instrument browser.\n\
         \n\
         Application:\n\
           menu\n\
           list\n\
           status\n\
           doctor\n\
           start PRESET\n\
           stop\n\
           log [LINES]\n\
           ideas list|inspect NAME|play NAME|delete NAME --yes\n\
         \n\
         Controller and routing:\n\
           pads list|ports|profiles|auto [PORT]|learn [PORT]|update\n\
           pads set NOTE ACTION|clear NOTE|input PORT_MATCH|layout 8|5|4\n\
           pads cc INCOMING TARGET\n\
           clock ports\n\
           casio diagnostic\n\
         \n\
         Configuration:\n\
           config paths\n\
           config init [--force]\n\
         \n\
         Non-audible maintenance:\n\
           screenshots\n\
           effects-checkpoint PRESET [PROFILE] [SECONDS]\n\
           recorder-stress DEST [SECONDS] [CHANNELS] [RATE] [CALLBACK]\n\
           final-mix-stress DEST [SECONDS] [RATE] [CALLBACK]\n\
         \n\
         Help: help, -h, --help\n\
         Compatibility aliases: logs; pads detect; casio status|dry-run;\n\
           phase2-checkpoint. Internal process command: daemon PRESET."
    );
}

fn clock_command(args: &[String], config: &config::RuntimeConfig) -> Result<()> {
    match args.first().map(String::as_str).unwrap_or("ports") {
        "ports" => {
            for name in
                crate::loop_player::controller_clock_outputs(&config.controller_clock.client_name)?
            {
                println!("current: {name}");
                println!("configure: {}", controller_learn::stable_input_match(&name));
            }
            Ok(())
        }
        other => bail!("unknown clock command {other:?}; use `shr clock ports`"),
    }
}

fn recorder_stress_command(args: &[String]) -> Result<()> {
    let destination = PathBuf::from(
        args.first()
            .context("usage: shr recorder-stress DEST [SECONDS] [CHANNELS] [RATE] [CALLBACK]")?,
    );
    let seconds = args
        .get(1)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(10);
    let channels = args
        .get(2)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(18);
    let sample_rate = args
        .get(3)
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(48_000);
    let callback_frames = args
        .get(4)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(128);
    if args.len() > 5 {
        bail!("usage: shr recorder-stress DEST [SECONDS] [CHANNELS] [RATE] [CALLBACK]");
    }
    let report = audio_recorder::run_synthetic_stress(
        &destination,
        Duration::from_secs(seconds),
        channels,
        sample_rate,
        callback_frames,
    )?;
    println!(
        "SYNTHETIC MULTITRACK PASS · {} channels · {} Hz · {} frames/callback",
        report.channels, report.sample_rate, report.callback_frames
    );
    println!(
        "FRAMES {} · elapsed {:.3} s · throughput {:.2} MiB/s",
        report.total_frames,
        report.elapsed.as_secs_f64(),
        report.throughput_bytes_per_second / 1_048_576.0
    );
    println!(
        "HIGH WATER {} frames · dropped {} · overflows {} · identity {}",
        report.writer_high_water_frames,
        report.dropped_frames,
        report.overflow_events,
        if report.channel_identity_verified {
            "verified"
        } else {
            "failed"
        }
    );
    println!("SESSION {}", report.session.display());
    Ok(())
}

fn final_mix_stress_command(args: &[String]) -> Result<()> {
    let destination = PathBuf::from(
        args.first()
            .context("usage: shr final-mix-stress DEST [SECONDS] [RATE] [CALLBACK]")?,
    );
    let seconds = args
        .get(1)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(10);
    let sample_rate = args
        .get(2)
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(48_000);
    let callback_frames = args
        .get(3)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(128);
    if args.len() > 4 {
        bail!("usage: shr final-mix-stress DEST [SECONDS] [RATE] [CALLBACK]");
    }
    let report = audio_recorder::run_final_mix_stress(
        &destination,
        Duration::from_secs(seconds),
        sample_rate,
        callback_frames,
    )?;
    println!(
        "SYNTHETIC FINAL MIX PASS · 3 stereo sources · {} Hz · {} frames/callback",
        report.sample_rate, report.callback_frames
    );
    println!(
        "FRAMES {} · elapsed {:.3}s · callback mean {:.3}us · p95 {:.3}us · p99 {:.3}us · max {:.3}us",
        report.total_frames,
        report.elapsed.as_secs_f64(),
        report.callback_mean_nanoseconds as f64 / 1000.0,
        report.callback_p95_nanoseconds as f64 / 1000.0,
        report.callback_p99_nanoseconds as f64 / 1000.0,
        report.callback_maximum_nanoseconds as f64 / 1000.0,
    );
    println!(
        "LIMITER MAX GR {:.2}dB · HIGH WATER {} frames · dropped {} · overflows {} · output/file {}",
        report.maximum_gain_reduction_db,
        report.writer_high_water_frames,
        report.dropped_frames,
        report.overflow_events,
        if report.output_file_equal { "equal" } else { "MISMATCH" }
    );
    println!("WAV {}", report.wav.display());
    if !report.output_file_equal {
        bail!("final playback samples and stereo WAV PCM differ");
    }
    Ok(())
}

fn casio_command(args: &[String], config: &config::RuntimeConfig) -> Result<()> {
    match args.first().map(String::as_str).unwrap_or("diagnostic") {
        "diagnostic" | "status" | "dry-run" => {
            print!("{}", sequencer::diagnostic(&config.external_midi)?);
            Ok(())
        }
        other => {
            bail!("unknown Casio command {other}; only non-transmitting diagnostic is available")
        }
    }
}

#[derive(Clone, Copy)]
struct DoctorCapability {
    label: &'static str,
    checks: usize,
    problems: usize,
}

impl DoctorCapability {
    const fn new(label: &'static str) -> Self {
        Self {
            label,
            checks: 0,
            problems: 0,
        }
    }

    fn record(&mut self, ok: bool) {
        self.checks += 1;
        if !ok {
            self.problems += 1;
        }
    }

    fn summary(self) -> String {
        if self.checks == 0 {
            format!("[--] {}: not configured", self.label)
        } else if self.problems == 0 {
            format!(
                "[ok] {}: {}/{} checks ready",
                self.label, self.checks, self.checks
            )
        } else {
            format!(
                "[!!] {}: {}/{} checks need attention",
                self.label, self.problems, self.checks
            )
        }
    }
}

fn doctor_check(
    problems: &mut usize,
    capability: &mut DoctorCapability,
    ok: bool,
    message: String,
) {
    println!("[{}] {message}", if ok { "ok" } else { "!!" });
    capability.record(ok);
    if !ok {
        *problems += 1;
    }
}

fn audio_tuning_doctor_target(cpu: Option<usize>) -> String {
    cpu.map(|value| value.to_string())
        .unwrap_or_else(|| "none".into())
}

fn doctor(config: &config::RuntimeConfig, preset_dir: &Path, state: &Path) -> Result<()> {
    let mut problems = 0;
    let mut core = DoctorCapability::new("CORE / EDITOR");
    let mut midi = DoctorCapability::new("MIDI");
    let mut audio = DoctorCapability::new("JACK AUDIO");
    let mut tuning = DoctorCapability::new("AUDIO TUNING");
    doctor_check(
        &mut problems,
        &mut core,
        command_exists(&config.synth_command),
        format!("synth command: {}", config.synth_command),
    );
    doctor_check(
        &mut problems,
        &mut audio,
        command_exists("jack_lsp"),
        "required command: jack_lsp".into(),
    );
    doctor_check(
        &mut problems,
        &mut midi,
        command_exists("aconnect"),
        "required command: aconnect".into(),
    );
    doctor_check(
        &mut problems,
        &mut core,
        preset_dir.is_dir(),
        format!("preset directory: {}", preset_dir.display()),
    );
    doctor_check(
        &mut problems,
        &mut core,
        state.join("shsynth.conf").is_file(),
        format!("runtime config: {}", state.join("shsynth.conf").display()),
    );
    let controller_path = state.join("controller.conf");
    let controller_exists = controller_path.is_file();
    doctor_check(
        &mut problems,
        &mut core,
        controller_exists,
        format!("controller config: {}", controller_path.display()),
    );
    let controller = if controller_exists {
        match pads::PadConfig::load(&controller_path) {
            Ok(controller) => {
                doctor_check(
                    &mut problems,
                    &mut core,
                    true,
                    "controller config parses".into(),
                );
                Some(controller)
            }
            Err(error) => {
                doctor_check(
                    &mut problems,
                    &mut core,
                    false,
                    format!("controller config is invalid: {error:#}"),
                );
                None
            }
        }
    } else {
        None
    };
    let jack = Command::new("jack_lsp").output().ok();
    let jack_ready = jack
        .as_ref()
        .map(|output| output.status.success())
        .unwrap_or(false);
    doctor_check(
        &mut problems,
        &mut audio,
        jack_ready,
        "JACK server reachable".into(),
    );
    if jack_ready && config.audio_autoconnect {
        let ports = String::from_utf8_lossy(&jack.as_ref().unwrap().stdout);
        for output in &config.audio_outputs {
            doctor_check(
                &mut problems,
                &mut audio,
                ports.lines().any(|port| port == output),
                format!("JACK output: {output}"),
            );
        }
    }
    if command_exists("shr-audio-tune") {
        let configured_cpu = audio_tuning_doctor_target(config.audio_engine_cpu);
        match Command::new("shr-audio-tune")
            .arg("doctor")
            .arg(configured_cpu)
            .output()
        {
            Ok(output) => {
                print!("{}", String::from_utf8_lossy(&output.stdout));
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
                doctor_check(
                    &mut problems,
                    &mut tuning,
                    output.status.success(),
                    "configured and live audio policy agree".into(),
                );
            }
            Err(error) => doctor_check(
                &mut problems,
                &mut tuning,
                false,
                format!("could not run shr-audio-tune doctor: {error}"),
            ),
        }
    } else if let Some(cpu) = config.audio_engine_cpu {
        doctor_check(
            &mut problems,
            &mut tuning,
            false,
            format!("audio.engine_cpu={cpu}, but shr-audio-tune is unavailable; reinstall SHR-DAW"),
        );
    } else {
        println!(
            "[--] dedicated audio CPU: optional and not configured; install shr-audio-tune to inspect"
        );
    }
    if config.midi_autoconnect {
        let default_controller = pads::PadConfig::default();
        let controller = controller.as_ref().unwrap_or(&default_controller);
        match engine::inspect_midi_inputs(config, controller) {
            Ok(availability) => {
                if let Some(controller) = availability.controller {
                    doctor_check(
                        &mut problems,
                        &mut midi,
                        controller.available(),
                        format!("controller MIDI: {}", controller.description()),
                    );
                } else {
                    println!("[--] controller MIDI: not configured");
                }
                if availability.performance.is_empty() {
                    println!(
                        "[--] performance MIDI: {}",
                        if config.midi_controller_musical_input {
                            "combined with controller/legacy input"
                        } else {
                            "not configured"
                        }
                    );
                }
                for performance in availability.performance {
                    doctor_check(
                        &mut problems,
                        &mut midi,
                        performance.available(),
                        format!(
                            "performance MIDI {}: {}",
                            performance.wanted,
                            performance.description()
                        ),
                    );
                }
            }
            Err(error) => doctor_check(
                &mut problems,
                &mut midi,
                false,
                format!("MIDI input discovery: {error:#}"),
            ),
        }
    }
    if config.controller_clock.enabled {
        match loop_player::controller_clock_outputs(&config.controller_clock.client_name) {
            Ok(names) => doctor_check(
                &mut problems,
                &mut midi,
                loop_player::matching_controller_output_index(
                    &names,
                    &config.controller_clock.output_match,
                )
                .is_ok(),
                format!(
                    "controller clock exact output: {}",
                    config.controller_clock.output_match
                ),
            ),
            Err(error) => doctor_check(
                &mut problems,
                &mut midi,
                false,
                format!("controller clock discovery: {error}"),
            ),
        }
    }
    println!("\nCapability summary");
    for capability in [core, midi, audio, tuning] {
        println!("{}", capability.summary());
    }
    if problems > 0 {
        bail!("doctor found {problems} problem(s)");
    }
    Ok(())
}

fn print_midi_input_status(config: &config::RuntimeConfig, state: &Path) {
    if !config.midi_autoconnect {
        println!("Controller MIDI: disabled");
        println!("Performance MIDI: disabled");
        return;
    }
    let controller = pads::PadConfig::load(&state.join("controller.conf")).unwrap_or_default();
    match engine::inspect_midi_inputs(config, &controller) {
        Ok(availability) => {
            println!(
                "Controller MIDI: {}",
                availability
                    .controller
                    .as_ref()
                    .map(engine::MidiInputState::description)
                    .unwrap_or_else(|| "not configured".into())
            );
            if availability.performance.is_empty() {
                println!(
                    "Performance MIDI: {}",
                    if config.midi_controller_musical_input {
                        "combined with controller/legacy input"
                    } else {
                        "not configured"
                    }
                );
            } else {
                for input in availability.performance {
                    println!("Performance MIDI {}: {}", input.wanted, input.description());
                }
            }
        }
        Err(error) => println!("MIDI input discovery unavailable: {error:#}"),
    }
}

fn command_exists(program: &str) -> bool {
    fsutil::command_exists(program)
}

fn config_command(args: &[String], state: &Path) -> Result<()> {
    match args.first().map(String::as_str).unwrap_or("paths") {
        "paths" => {
            println!("{}", state.join("shsynth.conf").display());
            println!("{}", state.join("controller.conf").display());
            Ok(())
        }
        "init" => {
            let runtime_path = state.join("shsynth.conf");
            let controller_path = state.join("controller.conf");
            let force = args.get(1).map(String::as_str) == Some("--force");
            if force || !runtime_path.exists() {
                config::RuntimeConfig::default().save(&runtime_path)?;
                println!("Created {}", runtime_path.display());
            } else {
                println!("Kept {}", runtime_path.display());
            }
            if force || !controller_path.exists() {
                pads::PadConfig::default().save(&controller_path)?;
                println!("Created {}", controller_path.display());
            } else {
                println!("Kept {}", controller_path.display());
            }
            Ok(())
        }
        other => bail!("unknown config command: {other}"),
    }
}

fn ideas_command(
    args: &[String],
    presets: &[preset::Preset],
    state: &Path,
    config: &config::RuntimeConfig,
) -> Result<()> {
    let base = recording::ideas_dir();
    match args.first().map(String::as_str).unwrap_or("list") {
        "list" => {
            for name in recording::list(&base)? {
                println!("{name}");
            }
            Ok(())
        }
        "inspect" => {
            let n = args.get(1).context("usage: shr ideas inspect NAME")?;
            print!("{}", recording::inspect(&base, n)?);
            Ok(())
        }
        "delete" => {
            let n = args.get(1).context("usage: shr ideas delete NAME --yes")?;
            if args.get(2).map(String::as_str) != Some("--yes") {
                bail!("deletion requires --yes");
            }
            recording::delete(&base, n)
        }
        "play" => {
            let n = args.get(1).context("usage: shr ideas play NAME")?;
            let (p, saved_values, events) = recording::load_with_parameters(&base, n)?;
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                signal_hook::flag::register(signal, std::sync::Arc::clone(&stop))?;
            }
            let mut values = engine::initial_values(&p)?;
            values.extend(saved_values);
            let (tx, _) = std::sync::mpsc::channel();
            let router = engine::MidiRouter::start(state, config, tx)?;
            if let Ok(mut backend) = router.backend().lock() {
                *backend = p.backend;
            }
            router.arm_pickup(&values);
            let mut engine = engine::Engine::start(&p, state, router.output(), config)?;
            engine.bind_midi_lifecycle(router.lifecycle());
            if engine.supports_parameter_reset() {
                engine.set_mapped_parameters(&values)?;
            }
            recording::play_events(
                &events,
                |m| {
                    let _ = engine.send(m);
                },
                stop.as_ref(),
            );
            drop(engine);
            Ok(())
        }
        other => {
            let _ = presets;
            bail!("unknown ideas command: {other}")
        }
    }
}

fn pads_command(args: &[String], state: &Path) -> Result<()> {
    let path = state.join("controller.conf");
    let mut config = pads::PadConfig::load(&path)?;
    match args.first().map(String::as_str).unwrap_or("list") {
        "ports" => {
            for name in controller_learn::input_names()? {
                println!("{name}");
            }
            Ok(())
        }
        "profiles" => {
            for profile in controller_profile::Catalog::discover().profiles() {
                println!("{}: {} [{}]", profile.id, profile.name, profile.source);
            }
            Ok(())
        }
        "auto" | "detect" => {
            let input = controller_learn::resolve_input(args.get(1).map(String::as_str))?;
            let stable_input = controller_learn::stable_input_match(&input);
            if let Some(profile) = controller_profile::Catalog::discover().matching(&input) {
                profile.apply(&mut config, &stable_input)?;
                if let Some(backup) = controller_learn::backup(&path)? {
                    println!("Backed up {}", backup.display());
                }
                config.save(&path)?;
                println!("Loaded known profile {} for {input}", profile.name);
            } else {
                config = pads::PadConfig::unmapped(stable_input);
                if let Some(backup) = controller_learn::backup(&path)? {
                    println!("Backed up {}", backup.display());
                }
                config.save(&path)?;
                println!("Selected {input}; no known profile. Run `shr pads learn`.");
            }
            Ok(())
        }
        "learn" => {
            let input = controller_learn::resolve_input(
                args.get(1)
                    .map(String::as_str)
                    .or(config.input_match.as_deref()),
            )?;
            if config.controls.is_empty() && config.pads.is_empty() && config.cc_buttons.is_empty()
            {
                if let Some(profile) = controller_profile::Catalog::discover().matching(&input) {
                    profile.apply(&mut config, &controller_learn::stable_input_match(&input))?;
                    println!("Started with known profile {}.", profile.name);
                }
            }
            controller_learn::learn(&mut config, &input)?;
            if let Some(backup) = controller_learn::backup(&path)? {
                println!("Backed up {}", backup.display());
            }
            config.save(&path)?;
            println!("Saved learned controller mapping to {}", path.display());
            Ok(())
        }
        "update" => update_controller_profiles(),
        "list" => {
            println!("input: {}", config.input_match.as_deref().unwrap_or("auto"));
            println!(
                "menu layout: {} buttons",
                match config.layout {
                    pads::ControllerLayout::Eight => 8,
                    pads::ControllerLayout::Five => 5,
                    pads::ControllerLayout::Four => 4,
                }
            );
            let encoder_turn = config.encoder_relative_cc.map_or_else(
                || "off".into(),
                |cc| {
                    format!(
                        "CC {cc} ({})",
                        if config.encoder_relative_reverse {
                            "reversed"
                        } else {
                            "normal"
                        }
                    )
                },
            );
            let encoder_press = config
                .encoder_press_cc
                .map(|cc| format!("CC {cc}"))
                .or_else(|| config.encoder_press_note.map(|note| format!("note {note}")))
                .unwrap_or_else(|| "off".into());
            let lock = config
                .lock_cc
                .map(|cc| format!("CC {cc}"))
                .unwrap_or_else(|| "off".into());
            println!("encoder: turn {encoder_turn}, press {encoder_press}; pad lock {lock}");
            if let (Some(modifier), Some(trigger)) =
                (config.page_cycle_modifier, config.page_cycle_trigger)
            {
                println!("page-cycle chord: hold {modifier}, trigger {trigger}");
            }
            let mut controls = config.controls.iter().collect::<Vec<_>>();
            controls.sort_by_key(|x| x.0);
            for (incoming, target) in controls {
                println!("cc {incoming} -> mapped CC {target}");
            }
            let mut v = config.pads.iter().collect::<Vec<_>>();
            v.sort_by_key(|x| x.0);
            for (n, a) in v {
                if let Some(channel) = config.pad_channels.get(n) {
                    println!("note {n}, channel {}: {a}", channel + 1);
                } else {
                    println!("note {n}, any channel: {a}");
                }
            }
            let mut v = config.cc_buttons.iter().collect::<Vec<_>>();
            v.sort_by_key(|x| x.0);
            for (cc, action) in v {
                if let Some(channel) = config.cc_button_channels.get(cc) {
                    println!("button CC {cc}, channel {}: {action}", channel + 1);
                } else {
                    println!("button CC {cc}, any channel: {action}");
                }
            }
            Ok(())
        }
        "set" => {
            let n = pads::midi_number(
                args.get(1).context("usage: shr pads set NOTE ACTION")?,
                "pad note",
            )?;
            let a = args
                .get(2)
                .context("usage: shr pads set NOTE ACTION")?
                .parse()?;
            config.pads.insert(n, a);
            config.pad_channels.remove(&n);
            config.save(&path)
        }
        "clear" => {
            let n = pads::midi_number(
                args.get(1).context("usage: shr pads clear NOTE")?,
                "pad note",
            )?;
            config.pads.remove(&n);
            config.pad_channels.remove(&n);
            config.save(&path)
        }
        "input" => {
            let name = args.get(1).context("usage: shr pads input PORT_MATCH")?;
            config.input_match = Some(name.clone());
            config.save(&path)
        }
        "layout" => {
            config.layout = match args.get(1).map(String::as_str) {
                Some("8" | "eight") => pads::ControllerLayout::Eight,
                Some("5" | "five") => pads::ControllerLayout::Five,
                Some("4" | "four") => pads::ControllerLayout::Four,
                _ => bail!("usage: shr pads layout 8|5|4"),
            };
            config.save(&path)
        }
        "cc" => {
            let incoming = pads::midi_number(
                args.get(1).context("usage: shr pads cc INCOMING TARGET")?,
                "controller CC",
            )?;
            let target: u8 = args
                .get(2)
                .context("usage: shr pads cc INCOMING TARGET")?
                .parse()?;
            if control::by_cc(target).is_none() {
                bail!("TARGET must be one of the 12 mapped CC numbers");
            }
            config.controls.insert(incoming, target);
            config.save(&path)
        }
        other => bail!("unknown pads command: {other}"),
    }
}

fn update_controller_profiles() -> Result<()> {
    let path = controller_profile::user_catalog_path();
    let parent = path.parent().context("controller profile directory")?;
    fs::create_dir_all(parent)?;
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            "1048576",
            controller_profile::UPDATE_URL,
        ])
        .output()
        .context("run curl to update controller profiles")?;
    if !output.status.success() {
        bail!("controller profile download failed");
    }
    let count = controller_profile::validate_catalog_bytes(&output.stdout)
        .context("validate downloaded controller profiles")?;
    fsutil::atomic_write(&path, &output.stdout)?;
    println!(
        "Installed {count} controller profiles at {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_keeps_the_original_index_selector_for_duplicate_names() {
        let presets = vec![
            preset::Preset::synthv1("Duplicate", PathBuf::from("first.synthv1")),
            preset::Preset::synthv1("Duplicate", PathBuf::from("second.synthv1")),
        ];
        let selected = preset::resolve(&presets, "preset_2").unwrap();
        assert_eq!(selected, &presets[1]);

        let reconstructed = format!("{}:{}", selected.backend.label(), selected.name);
        assert_eq!(preset::resolve(&presets, &reconstructed), Some(&presets[0]));
        assert_eq!(daemon_args("preset_2"), ["daemon", "preset_2"]);
    }

    #[test]
    fn effects_checkpoint_profiles_are_strict_and_cover_each_topology() {
        for profile in [
            "dry",
            "eq",
            "compressor",
            "soft-cubic",
            "hard-clip",
            "asymmetric",
            "gate",
            "filter-lp",
            "filter-bp",
            "filter-hp",
            "crusher",
            "full",
            "delay",
            "chorus",
            "flanger",
            "phaser",
            "tremolo",
            "autopan",
            "time-full",
            "reverb-room",
            "reverb-plate",
            "reverb-hall",
            "two-reverbs",
            "phase4-full",
        ] {
            let (rack, routing) = effects_routing(profile).unwrap();
            routing.validate(&rack).unwrap();
        }
        assert_eq!(effects_routing("full").unwrap().0.order.len(), 8);

        let (rack, routing) = effects_routing("phase4-full").unwrap();
        assert_eq!(rack.effects.len(), 8);
        assert_eq!(routing.buses.len(), 2);
        assert_eq!(routing.master_rack.effects.len(), 1);
        assert_eq!(
            rack.effects.len()
                + routing.master_rack.effects.len()
                + routing
                    .buses
                    .iter()
                    .map(|bus| bus.rack.effects.len())
                    .sum::<usize>(),
            11
        );
        assert!(effects_routing("future").is_err());
    }

    #[test]
    fn doctor_capability_summary_distinguishes_ready_partial_and_unconfigured() {
        let empty = DoctorCapability::new("AUDIO TUNING");
        assert_eq!(empty.summary(), "[--] AUDIO TUNING: not configured");

        let mut ready = DoctorCapability::new("CORE / EDITOR");
        ready.record(true);
        ready.record(true);
        assert_eq!(ready.summary(), "[ok] CORE / EDITOR: 2/2 checks ready");

        let mut partial = DoctorCapability::new("JACK AUDIO");
        partial.record(true);
        partial.record(false);
        assert_eq!(
            partial.summary(),
            "[!!] JACK AUDIO: 1/2 checks need attention"
        );
    }

    #[test]
    fn doctor_passes_configured_audio_intent_to_tuning_helper() {
        assert_eq!(audio_tuning_doctor_target(Some(3)), "3");
        assert_eq!(audio_tuning_doctor_target(None), "none");
    }
}
