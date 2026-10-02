//! Explicit paired-device actions, outside the UI and audio callback threads.
use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Device {
    pub address: String,
    pub name: String,
    pub connected: bool,
}

pub(crate) struct Completion {
    pub devices: Result<Vec<Device>, String>,
    pub action: Option<Result<(), String>>,
}

fn valid_address(address: &str) -> bool {
    address.len() == 17
        && address.split(':').count() == 6
        && address
            .split(':')
            .all(|part| part.len() == 2 && part.bytes().all(|c| c.is_ascii_hexdigit()))
}

fn command(args: &[&str]) -> Result<String> {
    // Coreutils timeout also bounds a stalled D-Bus call. No shell, scans,
    // pairing, trust changes, or implicit/default device selection.
    let output = Command::new("timeout")
        .args(["--kill-after=1", "12", "bluetoothctl", "--timeout", "10"])
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .context("Bluetooth needs bluetoothctl and timeout")?;
    if output.status.code() == Some(127) {
        bail!("Bluetooth needs bluetoothctl installed");
    }
    if !output.status.success() {
        bail!("Bluetooth command failed or timed out");
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_devices(paired: &str, connected: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in paired.lines() {
        let Some(rest) = line.strip_prefix("Device ") else {
            continue;
        };
        let Some((address, name)) = rest.split_once(' ') else {
            continue;
        };
        if !valid_address(address) || devices.iter().any(|d: &Device| d.address == address) {
            continue;
        }
        devices.push(Device {
            address: address.into(),
            name: name.chars().filter(|c| !c.is_control()).collect(),
            connected: connected.lines().any(|line| {
                line.strip_prefix("Device ")
                    .and_then(|s| s.split_once(' '))
                    .is_some_and(|(candidate, _)| candidate == address)
            }),
        });
    }
    devices
}

fn discover() -> Result<Vec<Device>> {
    Ok(parse_devices(
        &command(&["devices", "Paired"])?,
        &command(&["devices", "Connected"])?,
    ))
}

pub(crate) fn start(action: Option<(String, bool)>) -> Receiver<Completion> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let action = action.map(|(address, connect)| {
            (|| -> Result<()> {
                if !valid_address(&address) {
                    bail!("invalid Bluetooth device");
                }
                // Recheck the exact paired identity before changing any link.
                if !discover()?.iter().any(|d| d.address == address) {
                    bail!("Bluetooth device is no longer paired");
                }
                command(&[if connect { "connect" } else { "disconnect" }, &address])?;
                let devices = discover()?;
                if !devices
                    .iter()
                    .any(|d| d.address == address && d.connected == connect)
                {
                    bail!("Bluetooth link did not reach the requested state");
                }
                Ok(())
            })()
            .map_err(|error| error.to_string())
        });
        let devices = discover().map_err(|error| error.to_string());
        let _ = tx.send(Completion { devices, action });
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paired_identity_is_exact_and_keeps_disconnected_devices() {
        let paired = "Device 00:11:22:33:44:55 Room speaker\nDevice 00:11:22:33:44:66 Headphones\nDevice --bad Broken\n";
        let devices = parse_devices(paired, "Device 00:11:22:33:44:55 Room speaker\n");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "Room speaker");
        assert!(devices[0].connected);
        assert!(!devices[1].connected);
        assert!(!valid_address("--help"));
    }
}
