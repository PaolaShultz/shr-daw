# Codex-assisted SHR-DAW setup

Help me install, recover, or customize SHR-DAW on this Raspberry Pi.

Before acting, read `AGENTS.md`, `docs/WORKSPACE_HANDOFF.md`, `README.md`, and
the existing user configuration. Treat `install.sh` and `shr-setup` as the
supported path. This session should diagnose or customize that path, not
replace it with an undocumented machine-only workaround.

Inspect the operating system, dependencies, terminal geometry, ALSA MIDI
ports, JACK ports, audio interface, controller, and current SHR-DAW
configuration. Run the normal setup and checks where safe. If a repository
defect blocks setup, repair the project and add proportionate validation so the
normal installer works for the next person.

Give me one physical action at a time. Explain what it checks before asking me
to do it.

For controller discovery, listen without forwarding MIDI to a synth. Ask me to
move or press one control at a time. Identify the 12 continuous synth controls,
main relative encoder, encoder press, lock control, and command pads. Verify
the relative-encoder direction and value convention. Reject duplicate or
conflicting assignments, require Relative 1 or Relative 2, back up
`controller.conf`, and show
me the proposed map before calling it complete.

Help with JACK, ALSA, external-instrument, tracker-page, and SoundFont routing
when requested. Keep hardware names and routes in configuration, never in Rust
constants. Keep private downloads and user sound data outside the public
repository. Preserve source and licence notes, and never describe uncleared
material as redistributable.

Preserve existing configuration, presets, Ideas, Projects, recordings,
unrelated processes, and repository changes. Do not start or restart JACK,
launch an audible synth test, overwrite user data, publish, or make destructive
or system-wide changes without explaining the exact action and receiving my
permission. Never stop a synth process that SHR-DAW does not own. Back up files
before changing them, and keep repository repairs separate from private machine
settings.

Finish with proportionate non-audible validation. Run `shr doctor` only when
JACK is already available. Report what you found, changed, backed up, verified,
left untested, and what I should do next.
