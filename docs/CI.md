# Continuous integration

[The workflow](../.github/workflows/ci.yml) runs on pushes to `main`, pull
requests and manual dispatch. It uses read-only repository permissions, cancels
superseded runs and selects exact Rust 1.97.1 with locked Cargo dependencies.

The explicit request to add CI authorizes hosted locked checking and focused configuration contracts, not a local combined build pass. In accordance with AGENTS.md temporary restrictions, CI uses only config::tests:: on the shr binary plus existing isolated Python helper contracts. Complete Rust suites, Clippy and debug/release build campaigns are deliberately excluded. No local compilation was performed when adding this workflow.

The workflow contains the exact reproducible commands. Historical/exhaustive
auditions and benchmarks remain opt-in. No physical audio, MIDI, DMX, playback,
service activation, media download or deployment is part of these checks.
Compilation and synthetic tests do not establish Raspberry Pi hardware acceptance.
Clippy with warnings denied and release builds are not added as new CI gates.
