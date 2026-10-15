# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

Rust (edition 2024, stable), single dependency: `crossterm`. The `Makefile` wraps Cargo and passes `--target <host>` by default (override with `TARGET=<triple>`).

```sh
cargo run --release        # or: make run
cargo test                 # or: make test
cargo test <name>          # run a single test (tests live in `mod tests` at the bottom of src/main.rs)
cargo fmt --check          # make check = fmt --check + test
```

There is no linter config beyond `cargo fmt`.

## Architecture

The whole program is one file, `src/main.rs` (~925 lines): layout parsing, the `Trainer` state machine, the raw-mode terminal UI, and unit tests. Layouts are **data, not code** — adding one means adding files under `layouts/`, never editing Rust.

- **Layout files** (`layouts/<id>.layout`): `key=value` lines — `id`, `name`, `row=` (reference display), `words=<csv>`, and either `stages=a|b|c` (expanded to exactly 50 stages by repeating the last group) or exactly 50 `stage=` lines. `parse_layout_file` validates; `discover_layouts` scans at startup, sorts by display name, and collects errors to show in the UI. IDs must be lowercase ASCII/digits/hyphens and unique. Each layout points at its own comma-delimited word CSV in the same directory.
- **Data directory resolution** (`data_directory`): `ALT_LAYOUTS_DATA_DIR` env var → CWD if it contains `layouts/` → the executable's directory → its parent. Release packaging (`.github/workflows/release.yml`) ships the binary next to `layouts/`.
- **Progress persistence**: stored outside the repo in `~/.altlayouts-trainer/` (`HOME`, or `USERPROFILE` on Windows) — one `<layout-id>.tsv` per layout (`stage`, `mistake`, `seen` rows) plus `selected-layout`. Each layout has independent progress.
- **Training model**: 50 stages per layout; a stage unlocks after ≥12 attempts at ≥85% accuracy on its introduced keys. Missed keys get extra weight when generating Learning prompts. Stage 20 (`WORD_STAGE`) unlocks Practice words mode, which draws random dictionary words filtered to currently unlocked characters. The layout reference is only shown through stage `SHOW_LAYOUT_THROUGH_STAGE`.
- **Input is character-based**, not physical-key-based: the trainer compares typed characters to the prompt, so the user must set their OS keyboard layout to the one being practiced. Unicode (Cyrillic) is supported throughout, so key handling must treat `char`s, not bytes.
