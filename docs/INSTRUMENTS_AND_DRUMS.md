# Instruments and drums

SHR-DAW integrates five melodic instrument hosts and one independent drum
engine. This guide is the musician-facing home for choosing, loading, playing,
saving, routing, and recovering those sound sources. Machine paths and version
checks remain in [Configuration and routing](CONFIGURATION.md); process and
audio ownership remain in [How SHR-DAW works](HOW_IT_WORKS.md).

## What is integrated

| Sound source | Public format | SHR role | Sound editing and saving |
| --- | --- | --- | --- |
| synthv1 | `.synthv1` | One managed melodic host | Twelve mapped controls; private Overwrite or Save New |
| Yoshimi | `.xiz` | One managed melodic host | Catalog and playback are read-only in SHR |
| FluidSynth | `.sf2` / `.sf3` | One managed, multitimbral melodic host | Bank/program selection; SoundFonts remain read-only |
| Moj Sint | `.mojsint` | One managed melodic host with Model D and Six-Op PM | Twelve model-specific mapped controls; private Overwrite or Save New |
| SHR Sampler | `.shrinst` | One managed melodic sample-package host | Strict preloaded packages; read-only in SHR |
| SHR Drums | `.shrkit` | Independent in-process drum engine | Project kit, tuning, drum rack, and four tracker lanes |

Only one SHR-managed **melodic host process** runs at a time. Loading a new
synthv1, Yoshimi, FluidSynth, Moj Sint, or SHR Sampler sound safely replaces or
reuses that owner. FluidSynth may hold several compatible channel parts inside
its one process. SHR Drums is not part of that exclusive slot: it renders in
process and can play beside any managed melodic host.

## Browse, load, and switch safely

Open **Software Synths** from Home. Turn to browse sounds in the selected
catalog. Shift-turn the main encoder, use `[`/`]`, or click the two halves of
the Presets heading to move through all five melodic backends. Browsing is
silent. **LOAD** is the deliberate start or replacement boundary.

Loading follows one ownership transaction:

1. validate the selected sound and backend before disturbing the current one;
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

## Moj Sint

[Moj Sint](https://github.com/PaolaShultz/moj-sint) is SHR's editable in-house
synth host. Its 13 cleared factory starts are grouped by synthesis model:

- Model D: Full Bass, Full Lead, Full Filter Articulation, Matched Idealized,
  Matched Linear Mixer, Matched Linear Ladder, and Matched No Drift or
  Feedback;
- Six-Op PM: Bell Metal, Fractured Metal, Electric Piano Mallet, Glass Wood,
  Brass Bass, and Mechanical Stab.

Presets shows compact `M-D` and `6-OP` identities. In FT2 **ROUTE**, choosing
Moj Sint adds an explicit `ENGINE → MODEL → PATCH` hierarchy. Changing the
model selects that model's first available patch, and patch browsing never
crosses the selected model boundary. Apply keeps the complete live-auditioned
route; Cancel restores its opening snapshot.

Playback and FT2 **PARAM** use the same 12 physical control positions, with
labels chosen by the loaded model:

| Positions | Model D | Six-Op PM |
| --- | --- | --- |
| 1–4 | Evolve, Shape, Color, Edge | Index, Ratio, Feedback, Op Decay |
| 5–8 | Couple, Motion, Depth, Space | Balance, Key Scale, Velocity, Motion |
| 9–12 | Attack, Decay, Sustain, Release | Attack, Decay, Sustain, Release |

The stable CC 20–31 identity belongs to Moj Sint, not to synthv1 parameter
indices. After Load, Reset, Project/Idea restore, automation ownership changes,
or another value-setting transition, physical knobs must reach or cross the
effective value before they take control. This pickup rule prevents jumps.

**RESET** restores the loaded twelve values without restarting the synth.
**SAVE** offers Overwrite, Save New, and Cancel. Factory/system sounds are
read-only, so Overwrite redirects to the next private `User NNN` sound. Model D
and Six-Op PM keep separate private namespaces. A successful save becomes the
current sound and Reset baseline without releasing held notes; a failure keeps
the live sound and any previous file intact. A Moj Sint Idea carries its
private preset snapshot, while an FT2 route stores the model-qualified stable
sound identity.

## SHR Sampler

[SHR Sampler](https://github.com/PaolaShultz/shr-sampler) plays strict
`.shrinst` sample packages as a managed melodic host. The public installation
includes one cleared, project-authored neutral factory instrument. Packages
are read-only catalog entries: SHR does not rewrite a package or pretend it is
an editable synth preset.

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

## SHR Drums

[SHR Drums](https://github.com/PaolaShultz/shr-drums) is a bounded in-process
engine, not a sixth managed melodic process and not an external MIDI device.
It does not borrow synth or sampler voices. A new Project's four-lane **Drums**
page uses the installed Big Rock kit when available; an explicit external or
FluidSynth General MIDI drum route remains possible.

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
| Player | Load, edit, reset, save | Load and play read-only package | Not a Presets backend |
| Idea | MIDI plus private preset snapshot | MIDI plus stable package reference | Tracker workflow, not an Idea sound |
| FT2 route | Stable model and patch | Stable package identity | Stable kit or explicit MIDI/FluidSynth drum target |
| Automation | Twelve stable mapped controls | No invented mapped macros | Notes use drum lanes; Project drum effects use stable effect automation |
| Project audio | Managed stereo source | Managed stereo source | Independent in-process stereo source |

Project loading refuses unknown newer schemas rather than rewriting them.
Missing sounds and kits remain named and visibly unavailable; SHR never swaps
in a similarly named backend or route. Starting, stopping, route loss, live
switching, and reload retain the same note-cleanup and ownership boundaries.

## Where files and provenance live

Private user sounds, Projects, Ideas, and runtime state stay below the normal
XDG data roots or ignored `user/`. Public factory material is restricted to
the repository allowlists. Exact installed versions and commands are in
[Installation](INSTALLATION.md), machine settings are in
[Configuration and routing](CONFIGURATION.md), and redistribution evidence is
in [Third-party software and sounds](../THIRD_PARTY.md).

For step-by-step screens, continue with the
[screen and menu manual](MENU_MANUAL.md). For Pattern pages, route fields,
recording, and Arrangement behavior, use the [tracker guide](TRACKER.md).
