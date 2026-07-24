# Raspberry Pi 5 NVMe installation

This is the supported clean-storage path for the planned SHR-DAW Raspberry Pi
5. It installs a fresh official Raspberry Pi OS Lite 64-bit image directly on
the NVMe drive. Do not clone the Pi 4 microSD system or copy its tuning files.

This procedure is for a Raspberry Pi 5 NVMe drive connected through the PCIe
connector. A Raspberry Pi 4 has no equivalent native-NVMe connector; an NVMe
drive attached to a Pi 4 boots as USB mass storage and follows a different
path.

## Before writing the drive

You need:

- a Raspberry Pi 5, suitable power supply, active cooler, PCIe-to-NVMe adapter,
  and compatible NVMe drive;
- a temporary Raspberry Pi OS SD card **or** a USB NVMe enclosure connected to
  another computer; and
- Raspberry Pi Imager.

Disconnect power before fitting or removing the adapter, ribbon cable, or
drive. Install the active cooler first. Follow the adapter manufacturer's
ribbon orientation and mounting instructions: bottom-mounted adapters may not
match the photographs for Raspberry Pi's top-mounted M.2 HAT+.

The write operation below erases the selected NVMe completely. Disconnect
other removable drives when practical, and identify this one by both model and
capacity before accepting Imager's warning.

## Write Raspberry Pi OS Lite 64-bit

The shortest path is to put the NVMe in a USB enclosure and run Imager on
another computer. If no enclosure is available, use this staging path:

1. Use the familiar SD-card procedure to create a temporary Raspberry Pi OS
   **with Desktop** card. Boot the Pi 5 from it with the NVMe adapter installed.
2. Confirm that the NVMe is visible:

   ```sh
   lsblk -o NAME,SIZE,MODEL,TYPE,MOUNTPOINTS
   ls -l /dev/nvme*
   ```

   The whole drive normally appears as `/dev/nvme0n1`. Stop if the model or
   capacity is not the intended drive.
3. Install and open Raspberry Pi Imager:

   ```sh
   sudo apt update
   sudo apt install rpi-imager
   rpi-imager
   ```

Whether Imager runs on the Pi or another computer, make these selections:

1. **Device:** Raspberry Pi 5.
2. **OS:** Raspberry Pi OS Lite (64-bit).
3. **Storage:** the NVMe drive whose model and capacity were just confirmed.
4. **Customisation:** set the intended hostname, musician account, locale and
   timezone. Enable SSH and install the normal public key or password needed
   for headless access. Configure Wi-Fi if Ethernet will not be available.
5. Review the storage name once more, accept the erase warning, and let Imager
   finish both writing and verification. Do not skip verification.

For a reproducible acceptance record, note the Imager version, OS selection and
date. After first boot, record `/etc/os-release`, `uname -m`, `uname -r`, the
bootloader version, and the source revision installed. If a downloaded image
was selected with **Use Custom**, also retain its exact filename and checksum.

## First NVMe boot

1. Shut the staging system down cleanly:

   ```sh
   sudo poweroff
   ```

2. Disconnect power and remove the temporary SD card. Keep it unchanged until
   the NVMe boot has been verified.
3. Reconnect power. With no bootable SD card inserted, a Raspberry Pi 5 should
   boot automatically from an NVMe drive on a compatible M.2 adapter.
4. Connect over SSH and prove that this boot is using the NVMe:

   ```sh
   findmnt -no SOURCE /
   lsblk -o NAME,SIZE,MODEL,FSTYPE,MOUNTPOINTS
   uname -m
   cat /etc/os-release
   ```

   The root filesystem should resolve to an NVMe partition, normally
   `/dev/nvme0n1p2`, and `uname -m` must report `aarch64`. Do not continue with
   SHR-DAW installation if the root filesystem is still on the SD card.

Continue with the normal [SHR-DAW installation](INSTALLATION.md#install). Treat
the Pi 5 as a new machine: do not copy the Pi 4 boot command line, systemd
drop-ins, JACK configuration, runtime configuration, or Cargo output.

## If the Pi does not boot from NVMe

Keep changes narrow and test one cause at a time:

1. Reinsert the staging SD card, boot it, and rerun the `lsblk` and
   `/dev/nvme*` checks. If the NVMe is absent, power down and inspect the
   adapter, ribbon seating and orientation, drive seating, and power supply.
2. If the drive is visible, run:

   ```sh
   sudo raspi-config
   ```

   Choose **Advanced Options → Boot Order → NVMe/USB boot**, finish, and reboot.
3. If the bootloader is old, inspect it first:

   ```sh
   sudo rpi-eeprom-update
   ```

   Apply an available normal update with `sudo rpi-eeprom-update -a`, then
   reboot. Do not rewrite EEPROM configuration merely because NVMe exists.
4. Raspberry Pi HAT+ compliant adapters are discovered automatically. A custom
   or non-HAT+ adapter may require `dtparam=pciex1` in
   `/boot/firmware/config.txt` and `PCIE_PROBE=1` in EEPROM configuration.
   Apply those only when the exact adapter documentation requires them.
5. Leave PCIe at the supported Gen 2 speed. Raspberry Pi does not certify Pi 5
   PCIe Gen 3 operation, so `dtparam=pciex1_gen=3` is unsuitable as an
   installation default and especially unhelpful while diagnosing audio
   reliability.

Do not re-image repeatedly before checking the boot diagnostics, NVMe
visibility, adapter requirements, and boot order. Do not edit the NVMe's
`cmdline.txt` by hand to substitute device names; Raspberry Pi Imager writes
the partition identifiers needed by the image.

## Authoritative references

- Raspberry Pi's
  [M.2 HAT+ installation and NVMe boot procedure](https://www.raspberrypi.com/documentation/accessories/m2-hat-plus.html#boot-from-nvme).
- Raspberry Pi's
  [operating-system installation with Imager](https://www.raspberrypi.com/documentation/computers/getting-started.html#install-an-operating-system).
- Raspberry Pi's
  [NVMe boot order and PCIe configuration](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#boot-from-pcie).
