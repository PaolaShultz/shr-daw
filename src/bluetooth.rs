//! Bounded BlueZ work and durable reconnect intent, outside UI/audio threads.
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Device {
    pub address: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Link {
    Connect,
    Disconnect,
    Pair,
    Forget,
}
impl Link {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Connect => "CONNECT",
            Self::Disconnect => "DISCONNECT",
            Self::Pair => "PAIR+CONNECT",
            Self::Forget => "FORGET",
        }
    }
    pub(crate) fn cycle(self, direction: i8) -> Self {
        let choices = [Self::Connect, Self::Disconnect, Self::Pair, Self::Forget];
        choices[(choices.iter().position(|v| *v == self).unwrap() as isize + direction as isize)
            .rem_euclid(4) as usize]
    }
}

pub(crate) enum Action {
    #[cfg_attr(test, allow(dead_code))] // Routing discovery is hardware-free in UI tests.
    Refresh,
    Scan,
    Link(String, Link),
    Restore,
}
pub(crate) struct Completion {
    pub devices: Result<Vec<Device>, String>,
    pub action: Option<Result<(), String>>,
    pub remembered: Option<String>,
    pub background: bool,
}

fn valid_address(address: &str) -> bool {
    address.len() == 17
        && address.split(':').count() == 6
        && address
            .split(':')
            .all(|part| part.len() == 2 && part.bytes().all(|c| c.is_ascii_hexdigit()))
}

fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut escape = false;
    for c in text.chars() {
        if c == '\x1b' {
            escape = true;
            continue;
        }
        if escape {
            if c.is_ascii_alphabetic() {
                escape = false;
            }
            continue;
        }
        if !c.is_control() || c == '\n' {
            out.push(c);
        }
    }
    out
}

fn check_output(success: bool, output: &str) -> Result<()> {
    if let Some(line) = output.lines().find(|line| {
        line.contains("Failed")
            || line.contains("not available")
            || line.contains("No default controller")
            || line.contains("Authentication")
            || line.contains("org.bluez.Error")
            || line.contains("Invalid command")
    }) {
        if line.contains("br-connection-page-timeout") {
            bail!("No reply; power on or pair device");
        }
        if line.contains("No default controller") {
            bail!("Adapter unavailable; enable BT");
        }
        if line.contains("Authentication") {
            bail!("Pair failed; pairing mode/PIN needed");
        }
        bail!("{}", line.trim());
    }
    if !success {
        bail!("Bluetooth timed out or failed; check device power/pairing mode");
    }
    Ok(())
}

fn command(args: &[&str], seconds: u32, scanning: bool) -> Result<String> {
    let output = Command::new("timeout")
        .args([
            "--kill-after=1",
            &(seconds + 2).to_string(),
            "bluetoothctl",
            "--agent",
            "NoInputNoOutput",
            "--timeout",
            &seconds.to_string(),
        ])
        .args(args)
        .env("LC_ALL", "C")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .output()
        .context("Bluetooth needs bluetoothctl and timeout")?;
    let text = clean(&format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ));
    // bluetoothctl scan intentionally runs until its own timeout.
    check_output(
        output.status.success() || (scanning && output.status.code() == Some(124)),
        &text,
    )?;
    Ok(text)
}

fn parse_devices(known: &str, connected: &str, paired: &str) -> Vec<Device> {
    let contains = |text: &str, address: &str| {
        text.lines().any(|line| {
            line.trim()
                .strip_prefix("Device ")
                .and_then(|s| s.split_once(' '))
                .is_some_and(|(a, _)| a.eq_ignore_ascii_case(address))
        })
    };
    let mut devices = Vec::new();
    for line in known.lines() {
        let Some((address, name)) = line
            .trim()
            .strip_prefix("Device ")
            .and_then(|s| s.split_once(' '))
        else {
            continue;
        };
        if !valid_address(address)
            || devices
                .iter()
                .any(|d: &Device| d.address.eq_ignore_ascii_case(address))
        {
            continue;
        }
        devices.push(Device {
            address: address.to_ascii_uppercase(),
            name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
            connected: contains(connected, address),
            paired: contains(paired, address),
        });
        if devices.len() == 128 {
            break;
        }
    }
    devices
}

fn discover() -> Result<Vec<Device>> {
    discover_with(&mut command)
}
fn discover_with(
    run: &mut impl FnMut(&[&str], u32, bool) -> Result<String>,
) -> Result<Vec<Device>> {
    Ok(parse_devices(
        &run(&["devices"], 10, false)?,
        &run(&["devices", "Connected"], 10, false)?,
        &run(&["devices", "Paired"], 10, false)?,
    ))
}

fn remembered(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) if valid_address(text.trim()) => Ok(Some(text.trim().to_ascii_uppercase())),
        Ok(_) => bail!("Invalid saved Bluetooth device; connect a device again"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Read Bluetooth reconnect preference"),
    }
}

fn remember(path: &Path, address: Option<&str>) -> Result<()> {
    if address.is_some_and(|value| !valid_address(value)) {
        bail!("invalid Bluetooth device");
    }
    let parent = path.parent().context("Bluetooth state directory missing")?;
    std::fs::create_dir_all(parent)?;
    let temp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // A prior power loss can leave only our fixed private temporary file.
    if temp.exists() {
        std::fs::remove_file(&temp)?;
    }
    let mut file = options.open(&temp)?;
    writeln!(file, "{}", address.unwrap_or(""))?;
    file.sync_all()?;
    std::fs::rename(&temp, path)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn link(address: &str, action: Link, path: &Path) -> Result<()> {
    link_with(address, action, path, &mut command)
}
fn link_with(
    address: &str,
    action: Link,
    path: &Path,
    run: &mut impl FnMut(&[&str], u32, bool) -> Result<String>,
) -> Result<()> {
    if !valid_address(address) {
        bail!("invalid Bluetooth device");
    }
    // Persist the user's stop intent before touching BlueZ, including failed disconnects.
    if matches!(action, Link::Disconnect | Link::Forget)
        && remembered(path)?.as_deref() == Some(address)
    {
        remember(path, None)?;
    }
    match action {
        Link::Pair => {
            // BlueZ pair removes an existing pairing; preserve existing bonds.
            if !discover_with(run)?
                .iter()
                .any(|d| d.address == address && d.paired)
            {
                run(&["pair", address], 45, false)?;
            }
            run(&["trust", address], 10, false)?;
            // Recent BlueZ pair already connects; avoid treating AlreadyConnected as failure.
            if !discover_with(run)?
                .iter()
                .any(|d| d.address == address && d.connected)
            {
                run(&["connect", address], 15, false)?;
            }
        }
        Link::Connect => {
            run(&["connect", address], 15, false)?;
            run(&["trust", address], 10, false)?;
        }
        Link::Disconnect => {
            run(&["disconnect", address], 10, false)?;
        }
        Link::Forget => {
            run(&["remove", address], 10, false)?;
        }
    }
    let devices = discover_with(run)?;
    let device = devices.iter().find(|d| d.address == address);
    let reached = match action {
        Link::Connect => device.is_some_and(|d| d.connected),
        Link::Pair => device.is_some_and(|d| d.connected && d.paired),
        Link::Disconnect => device.is_none_or(|d| !d.connected),
        Link::Forget => device.is_none_or(|d| !d.paired),
    };
    if !reached {
        bail!("Bluetooth did not reach requested state");
    }
    if matches!(action, Link::Connect | Link::Pair) {
        remember(path, Some(address)).context("Connected, but saving reconnect failed")?;
    }
    Ok(())
}

fn restore_with(
    path: &Path,
    run: &mut impl FnMut(&[&str], u32, bool) -> Result<String>,
) -> Result<()> {
    let Some(address) = remembered(path)? else {
        return Ok(());
    };
    let devices = discover_with(run)?;
    let Some(device) = devices.iter().find(|d| d.address == address && d.paired) else {
        bail!("Saved device unpaired; select and pair");
    };
    if !device.connected {
        run(&["connect", &address], 15, false)?;
    }
    Ok(())
}

pub(crate) fn start(action: Action, state: PathBuf) -> Receiver<Completion> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let path = state.join("bluetooth-reconnect");
        // No saved intent means startup has no Bluetooth work or adapter side effects.
        if matches!(action, Action::Restore) && matches!(remembered(&path), Ok(None)) {
            let _ = tx.send(Completion {
                devices: Ok(Vec::new()),
                action: None,
                remembered: None,
                background: true,
            });
            return;
        }
        let background = matches!(action, Action::Restore);
        let quiet = matches!(action, Action::Refresh | Action::Restore);
        let result = (|| -> Result<()> {
            match action {
                Action::Refresh => (),
                Action::Scan => {
                    command(&["power", "on"], 10, false)?;
                    command(&["scan", "on"], 10, true)?;
                }
                Action::Link(address, action) => link(&address, action, &path)?,
                Action::Restore => restore_with(&path, &mut command)?,
            }
            Ok(())
        })()
        .map_err(|error| format!("{error:#}"));
        let devices = discover().map_err(|error| error.to_string());
        let _ = tx.send(Completion {
            devices,
            action: if quiet && result.is_ok() {
                None
            } else {
                Some(result)
            },
            remembered: remembered(&path).ok().flatten(),
            background,
        });
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_includes_unpaired_and_preserves_exact_identity() {
        let all = "Device 00:11:22:33:44:55 Speaker\nDevice 00:11:22:33:44:66 New headphones\nDevice --bad Broken\nDevice 00:11:22:33:44:55 duplicate\n";
        let devices = parse_devices(
            all,
            "Device 00:11:22:33:44:55 Speaker",
            "Device 00:11:22:33:44:55 Speaker",
        );
        assert_eq!(devices.len(), 2);
        assert!(devices[0].paired);
        assert!(devices[0].connected);
        assert!(!devices[1].paired);
        assert!(!devices[1].connected);
        assert!(!valid_address("--help"));
    }
    #[test]
    fn bluez_failure_with_successful_process_exit_is_not_success() {
        assert!(check_output(
            true,
            "Failed to connect: org.bluez.Error.Failed br-connection-page-timeout"
        )
        .is_err());
        assert!(check_output(true, "No default controller available").is_err());
        assert!(check_output(true, "Connection successful").is_ok());
        assert_eq!(
            clean("\x1b[0;94mDevice 00:11:22:33:44:55 BT\x1b[0m"),
            "Device 00:11:22:33:44:55 BT"
        );
    }
    #[test]
    fn successful_choice_and_explicit_stop_survive_reload() {
        let root = std::env::temp_dir().join(format!(
            "shr-bt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("bluetooth-reconnect");
        assert_eq!(remembered(&path).unwrap(), None);
        remember(&path, Some("00:11:22:33:44:55")).unwrap();
        assert_eq!(
            remembered(&path).unwrap().as_deref(),
            Some("00:11:22:33:44:55")
        );
        std::fs::write(path.with_extension("tmp"), "interrupted write").unwrap();
        remember(&path, None).unwrap();
        assert_eq!(remembered(&path).unwrap(), None);
        assert!(remember(&path, Some("--help")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn link_transactions_save_only_success_and_clear_stop_before_failure() {
        let root = std::env::temp_dir().join(format!(
            "shr-bt-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("bluetooth-reconnect");
        let address = "00:11:22:33:44:55";
        let other = "00:11:22:33:44:66";
        remember(&path, Some(other)).unwrap();
        let mut fail = |_: &[&str], _: u32, _: bool| -> Result<String> { bail!("not answering") };
        assert!(link_with(address, Link::Connect, &path, &mut fail).is_err());
        assert_eq!(remembered(&path).unwrap().as_deref(), Some(other));
        let mut commands = Vec::new();
        let mut success = |args: &[&str], _: u32, _: bool| -> Result<String> {
            commands.push(args.join(" "));
            Ok(if args[0] == "devices" {
                format!("Device {address} Speaker")
            } else {
                String::new()
            })
        };
        link_with(address, Link::Pair, &path, &mut success).unwrap();
        // PAIR on an existing paired device must not destroy/recreate its bond.
        assert!(!commands.iter().any(|cmd| cmd.starts_with("pair ")));
        assert!(commands.contains(&format!("trust {address}")));
        assert_eq!(remembered(&path).unwrap().as_deref(), Some(address));
        assert!(link_with(address, Link::Disconnect, &path, &mut fail).is_err());
        assert_eq!(remembered(&path).unwrap(), None);
        remember(&path, Some(address)).unwrap();
        assert!(link_with(address, Link::Forget, &path, &mut fail).is_err());
        assert_eq!(remembered(&path).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_without_saved_choice_does_no_device_work() {
        let root = std::env::temp_dir().join(format!("shr-bt-absent-{}", std::process::id()));
        let completion = start(Action::Restore, root)
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(completion.devices.unwrap().is_empty());
        assert!(completion.action.is_none());
        assert!(completion.remembered.is_none());
    }
    #[test]
    fn restore_requires_exact_still_paired_identity() {
        let root = std::env::temp_dir().join(format!(
            "shr-bt-restore-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("bluetooth-reconnect");
        let address = "00:11:22:33:44:55";
        remember(&path, Some(address)).unwrap();
        for (known, paired, connected, connects, succeeds) in [
            ("", "", "", 0, false),
            (
                "Device 00:11:22:33:44:66 Other",
                "Device 00:11:22:33:44:66 Other",
                "",
                0,
                false,
            ),
            ("Device 00:11:22:33:44:55 Speaker", "", "", 0, false),
            (
                "Device 00:11:22:33:44:55 Speaker",
                "Device 00:11:22:33:44:55 Speaker",
                "",
                1,
                true,
            ),
            (
                "Device 00:11:22:33:44:55 Speaker",
                "Device 00:11:22:33:44:55 Speaker",
                "Device 00:11:22:33:44:55 Speaker",
                0,
                true,
            ),
        ] {
            let mut count = 0;
            let result = restore_with(&path, &mut |args, _, _| {
                Ok(match args {
                    ["devices"] => known.to_string(),
                    ["devices", "Paired"] => paired.to_string(),
                    ["devices", "Connected"] => connected.to_string(),
                    ["connect", target] => {
                        assert_eq!(*target, address);
                        count += 1;
                        String::new()
                    }
                    _ => panic!("unexpected Bluetooth action: {args:?}"),
                })
            });
            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(count, connects);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn new_device_pairs_trusts_and_remembers_without_duplicate_connect() {
        let root = std::env::temp_dir().join(format!(
            "shr-bt-pair-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("bluetooth-reconnect");
        let address = "00:11:22:33:44:55";
        let line = format!("Device {address} New speaker");
        let mut paired = false;
        let mut trusted = false;
        link_with(address, Link::Pair, &path, &mut |args, _, _| {
            Ok(match args {
                ["devices"] => line.clone(),
                ["devices", "Paired" | "Connected"] => {
                    if paired {
                        line.clone()
                    } else {
                        String::new()
                    }
                }
                ["pair", target] => {
                    assert_eq!(*target, address);
                    assert!(!paired);
                    paired = true;
                    String::new()
                }
                ["trust", target] => {
                    assert_eq!(*target, address);
                    assert!(paired);
                    trusted = true;
                    String::new()
                }
                _ => panic!("unexpected action {args:?}"),
            })
        })
        .unwrap();
        assert!(paired && trusted);
        assert_eq!(remembered(&path).unwrap().as_deref(), Some(address));
        std::fs::remove_dir_all(root).unwrap();
    }
}
