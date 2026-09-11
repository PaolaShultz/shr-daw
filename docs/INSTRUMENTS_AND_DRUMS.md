# SHR-DAW instruments and drums

The installed SHR-DAW package is one music workstation with a complete sound
system: five melodic instrument families, the SHR Drums instrument and kits,
one controller workflow, one tracker, one effects graph, and one final audio
bus. Moj Sint, SHR Sampler, and SHR Drums arrive and work together as parts of
SHR-DAW. Their names identify kinds of sound available inside the workstation.

This guide is the musician-facing home for choosing, loading, playing, saving,
routing, and recovering SHR-DAW sounds. Machine paths and component version
checks remain in [Configuration and routing](CONFIGURATION.md); process and
audio ownership remain in [How SHR-DAW works](HOW_IT_WORKS.md).

## The SHR-DAW sound system

| Instrument family | Sounds inside SHR-DAW | Musical role | Editing and saving |
| --- | --- | --- | --- |
| synthv1 | `.synthv1` sounds | Melodic synth | Twelve mapped controls; private Overwrite or Save New |
| Yoshimi | `.xiz` sounds and banks | Melodic synth | Volume and Project AUX sends; preset files remain read-only |
| FluidSynth | `.sf2` / `.sf3` SoundFonts | Multitimbral melodic or General MIDI drums | Bank/program selection, Volume, and shared stereo AUX sends; SoundFonts remain read-only |
| Moj Sint | `.mojsint` Model D, Six-Op PM, Strange Oscillator, Swarm Machine, Bass Matrix, Dual Filter, Pressure Chain, and Open303 sounds | Melodic synth | Model-specific controls and Project AUX sends; Dual Filter adds a reversible core click; private Overwrite or Save New |
| SHR Sampler | `.shrinst` instruments | Melodic sample instrument | Strict preloaded instruments, Volume, and Project AUX sends; packages remain read-only |
| SHR Drums | `.shrkit` kits | Four-lane drum instrument | Project kit, tuning, drum rack, and tracker notes |

All six families participate in the same Project, routes, effects, transport,
recording, controller, and final-bus workflows. At the implementation boundary,
only one SHR-managed **melodic host process** runs at a time. Loading a new
synthv1, Yoshimi, FluidSynth, Moj Sint, or SHR Sampler sound safely replaces or
reuses that owner. FluidSynth may hold several compatible channel parts inside
its one process. SHR Drums renders in process and can play beside the selected
melodic instrument. This arrangement keeps drum audio independent inside the
same installation and workflow.

## Browse, load, and switch safely

Open **Software Synths** from Home. Turn to browse sounds in the selected
catalog. Shift-turn the main encoder, use `[`/`]`, or click the two halves of
the Presets heading to move through all five melodic instrument families.
Browsing is silent. **LOAD** is the deliberate start or replacement boundary.

Loading follows one ownership transaction:

1. validate the selected sound and its engine before disturbing the current
   one;
2. send All Notes Off and stop only the melodic process SHR owns when a
   replacement is required;
3. start or reuse the selected backend;
4. resolve its exact MIDI input and stereo JACK outputs; and
5. publish the new sound only after the route is ready.

A failed replacement leaves no second managed engine layered. When possible,
SHR makes one bounded attempt to restore the previous owned session and shows
the fault. **PANIC**, shutdown, or another explicit replacement releases notes
and stops only owned work; it never terminates a matching process opened by the
musician.

Presets and Playback share the loaded sound. Leaving either screen does not
stop it. A new, empty, unsaved FT2 Project can adopt that exact Player sound on
its first software page without restarting the host. A saved or already edited
Project keeps its stored routes.

## Moj Sint sounds

Moj Sint is SHR-DAW's editable in-house synthesis family. The installer pins
a 28-start catalog with these eight models:

- Model D: Full Bass, Full Lead, Full Filter Articulation, Matched Idealized,
  Matched Linear Mixer, Matched Linear Ladder, and Matched No Drift or
  Feedback;
- Six-Op PM: Bell Metal, Fractured Metal, Electric Piano Mallet, Glass Wood,
  Brass Bass, and Mechanical Stab;
- Strange Oscillator: one unified sound whose TYPE control selects triangle,
  saw, pulse, modulated resonator, deformed loop, stochastic breakpoints,
  scanned string, or register machine;
- Swarm Machine: the typed modular graph's warm, wide nine-oscillator pad;
- Bass Matrix: one transformable start with a phase-locked sub/body and a
  separate punch, growl, metal, drive, filter, and unstable character path.

- Dual Filter: Industrial Lead, Serial Bass, Counter Growl, Envelope Punch,
  and Topology Motion;
- Pressure Chain: Deep Cascade, Body Tap, and Cross Feed. Each preset selects
  one monophonic topology with velocity-coupled pressure and overlapping-note
  slide. Releasing the latest note returns to the most recently held note.

- Open303: Rubber Bass, Accent Wire, Hollow Slide, and Soft Pluck, with native
  acid filter and accent/slide envelope controls.

Pressure Chain requires Moj Sint preset schema 9; Open303 requires schema 10. The installation pin contains
that engine and all 28 cleared starts. Existing installations need a matching
host binary update before loading the new model; source changes alone do not
replace a running or installed executable.

Presets groups the available catalog in the fixed model order Model D, Six-Op
PM, Strange Oscillator, Swarm Machine, Bass Matrix, Dual Filter, Pressure Chain, then Open303. Visible identities use
one model letter and a two-digit number local to that model: `D01`, `P01`,
`O01`, `S01`, `B01`, `F01`, `C01`, and `A01`. Opening or switching to Moj Sint starts at
`D01 Full Bass`; letter-jump follows those visible model letters. In FT2
**ROUTE**, choosing Moj Sint adds an explicit `ENGINE → MODEL → PATCH`
hierarchy. Changing the model selects that model's first available patch, and
patch browsing never crosses the selected model boundary. Apply keeps the
complete live-auditioned route; Cancel restores its opening snapshot.

The Presets list, Playback, and FT2 **PARAM** show one inverted `M` cell beside
Pressure Chain and Open303 sounds: these models play one note at a time.
Long names leave room for the marker. The other Moj models have no voice marker.

Playback and FT2 **PARAM** share a **4×4** surface matching physical rotaries
1–16. The first two rows hold tone controls, the third holds envelopes, and
the last is always **Volume / AUX 1 / AUX 2 / AUX 3**. Rotary 1 normally edits
the first parameter. Click it to enter visible **NAV**, turn to select a
controller menu page, and click again to resume editing. Pads still activate
the visible actions. Leaving the screen or loading a sound returns to editing.
RESET remains an explicit SOUND action; clicking the rotary no longer resets.
Existing learned mappings remain valid; no Learn pass is needed.

The one information row below the grid shows the chord and held note names,
plus the selected scale while N00B is enabled. Strike velocities and the
keyboard graphic no longer compete for space. In Player, Shift-turn changes
the N00B scale; ordinary turns keep editing the first parameter.

| Model | Rotaries 1–4 | Rotaries 5–8 | Rotaries 9–12 |
| --- | --- | --- | --- |
| Model D | Character, Osc Mix, Cutoff, Drive | Couple, F Env, Ladder, Resonance | Attack, Decay, Sustain, Release |
| Six-Op PM | Index, Ratio, Feedback, Op Decay | Balance, KeyScale, Velocity, Motion | Attack, Decay, Sustain, Release |
| Strange Oscillator | Type, Form, Warp, Couple | Motion, Chaos, Color, Space | Attack, Decay, Sustain, Release |
| Swarm Machine | Mass, Detune, Spread, Shape | Bite, Motion, Color, Space | Attack, Decay, Sustain, Release |
| Bass Matrix | Body, Growl, Metal, Punch | Drive, Filter, Unstable, — | Attack, Decay, Sustain, Release |
| Dual Filter | A Cutoff, A Res, A Env, B Cutoff | B Res, B Env, Struct, F Decay | A Attack, A Decay, A Sus, A Rel |
| Pressure Chain | Source, Shape, Cutoff, Res | Sweep, F Decay, Pressure, Bite | Attack, Decay, Sustain, Release |
| Open303 | Wave, Cutoff, Res, Env Mod | Accent, Slide, Ac Atk, — | F Attack, F Decay, Ac Decay, Amp Dec |
| synthv1 | Flt cut, Flt res, Flt env, LFO rate | Dly amt, Dly time, Dly fb, — | Atk, Dec, Sus, Rel |

Every row above is followed by **Volume / AUX 1 / AUX 2 / AUX 3** on rotaries
13–16. Open303 retains its own envelope timings instead of generic ADSR.
Blank positions do nothing. Static parameter labels must fit nine terminal
cells within each ten-cell column; saved parameter names and native CCs remain
independent of display spelling.

The surface audit restored the native controls displaced by Volume: Model D
feedback (`Couple`), Six-Op balance, Strange motion and Swarm bite. Model D's
old `Space` label actually controlled resonance; `Character`, `Osc Mix`,
`Cutoff`, `Drive`, `F Env`, `Ladder` and `Resonance` now describe their DSP roles.
Bass Matrix's historical CC24 is unused by its live macro path, so no placeholder
knob is exposed. Open303 and synthv1 likewise leave their spare cell empty.

Dual Filter needs fifteen native tone/envelope controls plus Volume. Its live
surface prioritizes both filters' cutoff, resonance and envelope amount,
structure, filter decay and the complete amp envelope. Filter attack, sustain
and release are preset-owned detail controls; removing them from the surface
eliminates the AMP/FILTER page switch. Their loaded values are still retained
by Save, Reset and automation. This is a performance-surface choice, not deletion
of the DSP or saved fields. The audit follows each companion model's live
control mapping and SHR's preset CC-to-field mapping. Automation resolves names
against the page's model, so shared names such as `motion` reach that model's
own CC instead of the first similarly named control in another model.

Yoshimi, FluidSynth, and SHR Sampler expose only the final Volume/AUX row.
FluidSynth's sends process its whole shared stereo mix. External MIDI instruments
need a configured audio return for SHR effects. AUX sends are consumed inside
SHR, start OFF, and retain absolute pickup and recording guards.

Volume uses standard CC7 on Moj and the optional managed hosts; synthv1 uses
its own DCA control. Moj's smoothed volume runs from silence to the preset's
normal maximum without entering timbre DSP. Optional hosts' preset files remain
read-only; Project MIDI state and FT2 automation own durable volume. Relative
turns continue from the effective value after load, reset and automation changes.

**RESET** restores the loaded model values without restarting the synth. Dual
Filter also restores its saved INDUSTRIAL or COUNTER core.
**SAVE** offers Overwrite, Save New, and Cancel. Factory/system sounds are
read-only, so Overwrite redirects to the next private `User NNN` sound. All
eight Moj models keep separate private namespaces. A successful save becomes the
current sound and Reset baseline without releasing held notes; a failure keeps
the live sound and any previous file intact. A Moj Sint Idea carries its
private preset snapshot, while an FT2 route stores the model-qualified stable
sound identity.

## SHR Sampler instruments

SHR Sampler is the sample-instrument family inside SHR-DAW. It plays strict
`.shrinst` packages, and the installation includes one cleared,
project-authored neutral factory instrument. Packages are read-only catalog
entries. SHR treats each package as a complete instrument.

**LOAD** first checks the installed host version and runs the package's bounded
offline validation. Only a compatible, valid package may replace the current
melodic owner. The live host must then publish its exact configured MIDI input
and stereo JACK outputs. A missing executable or package, incompatible
version, malformed manifest, validation timeout, missing ports, startup
failure, or unexpected exit becomes a visible fault. The previous owned
session is not discarded until validation succeeds, and failed activation
cannot leave a second melodic process running.

Playback provides notes, held-note/velocity feedback, N00B filtering, take
capture, effects access, and transport without inventing unsupported Sampler
macros. Sound saving is visibly unavailable. FT2 stores the package's stable
identity in its software route. Ideas store that identity and configured
public path; they do not copy sample content into the private Idea directory.

## SHR Drums kits

SHR Drums is SHR-DAW's kit-based drum instrument. Its bounded engine runs
inside SHR-DAW, with its own voices alongside the current melodic instrument.
A new Project's four-lane **Drums** page uses the installed Big Rock kit when
available; an explicit external or FluidSynth General MIDI drum route remains
possible.

The public installation contains four cleared kits:

- Acid, an original fully modelled CC0 kit;
- Electronic House, original modelled voices plus two deterministic CC0 Moj
  Sint one-shot exports;
- Big Rock, a curated CC BY 4.0 acoustic kit; and
- Experimental Noise, a curated CC BY 4.0 experimental kit.

Open FT2 **ROUTE** on a Drums page and choose `TARGET → SHR Drums → KIT`.
Kit changes are live-auditioned inside the same Apply/Cancel transaction as
other routes. A successful change resets tuning that belonged to the previous
kit while keeping the Project key and drum effects. A failed load restores the
previous kit and keeps the route editor open with the error visible.

The Project stores the selected kit, `OFF`, `FOLLOW KEY`, or `MANUAL` per-piece
tuning, and the fixed Reverb-then-Delay drum rack. Follow Key uses the Project
tonic. The Drum page's four columns remain independent tracker lanes, and
loading a reusable drum pattern copies note cells without replacing the saved
kit or route. Immediate chokes and note cleanup apply when a drum target or kit
changes.

With the owned audio graph active, SHR Drums has its own final-bus source
level, mute, meter, and `DRUMS` effect target before the master rack and fixed
master strip. Without that graph, its owned stereo output follows the direct
JACK playback path. The metronome remains a separate final-bus sound and never
borrows a drum voice.

## Projects, Ideas, and automation

| Context | Moj Sint | SHR Sampler | SHR Drums |
| --- | --- | --- | --- |
| Player | Load, edit, reset, save | Load and play read-only package | FT2 Drums page |
| Idea | MIDI plus private preset snapshot | MIDI plus stable package reference | Tracker workflow, not an Idea sound |
| FT2 route | Stable model and patch | Stable package identity | Stable kit or explicit MIDI/FluidSynth drum target |
| Automation | Seven timbre controls, shared volume, and ADSR | Standard channel volume plus note performance | Notes use drum lanes; Project drum effects use stable effect automation |
| Project audio | Managed stereo source | Managed stereo source | Independent in-process stereo source |

Project loading refuses unknown newer schemas rather than rewriting them.
Missing sounds and kits remain named and visibly unavailable; SHR never swaps
in a similarly named backend or route. Starting, stopping, route loss, live
switching, and reload retain the same note-cleanup and ownership boundaries.
External USB MIDI sync changes only the steady transport owner: incoming
Start/Stop still enter these same managed-instrument, drum, held-note, Loop,
and cleanup paths, and no instrument route receives forwarded clock. See
[Priority 7 external transport sync acceptance](EXTERNAL_TRANSPORT_SYNC_ACCEPTANCE.md).

## Where files and provenance live

Private user sounds, Projects, Ideas, and runtime state stay below the normal
XDG data roots or ignored `user/`. Public factory material is restricted to
the repository allowlists. SHR-DAW installs and operates the system as one
workstation, while its component source and sound formats remain public:

- [Moj Sint source and preset format](https://github.com/PaolaShultz/moj-sint);
- [SHR Sampler source and instrument format](https://github.com/PaolaShultz/shr-sampler);
- [SHR Drums source, kit format, and provenance](https://github.com/PaolaShultz/shr-drums).

Exact installed versions and commands are in [Installation](INSTALLATION.md),
machine settings are in [Configuration and routing](CONFIGURATION.md), and
redistribution evidence is in
[Third-party software and sounds](../THIRD_PARTY.md).

For step-by-step screens, continue with the
[screen and menu manual](MENU_MANUAL.md). For Pattern pages, route fields,
recording, and Arrangement behavior, use the [tracker guide](TRACKER.md).
