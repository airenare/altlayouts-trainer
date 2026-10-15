# AltLayouts Trainer

A cross-platform terminal typing trainer that gradually introduces alternative keyboard layouts and revisits keys you miss more often. Layouts are loaded from data files, so adding one does not require editing or recompiling the Rust source.

## Requirements

- Rust and Cargo (stable)
- Set your operating system's keyboard layout to the selected layout for physical-key practice. The trainer compares the character you type with the prompted character; it does not inspect physical key positions or change your OS layout.

## Run

```sh
cargo run --release
```

Or use the `Makefile` targets (`make build`, `make release`, `make run`, `make test`, `make fmt`, `make check`, `make clean`). Install Rust and Cargo first; GNU Make is optional if you use the Cargo commands directly. Keep the executable with its `layouts/` folder and word CSV files, or set `ALT_LAYOUTS_DATA_DIR` to a data directory containing `layouts/`. `make build` uses the host Rust target by default; override it with `make release TARGET=<rust-target-triple>` after installing that target with rustup. For a portable install, copy the executable together with `layouts/` into the same directory.

Use `[l] Next layout` to cycle through automatically discovered layouts. Built-in examples include Colemak, Colemak-DH, Dvorak, Workman, and Russian Phonetic (ChromeOS-style). Each layout has separate progress. **Learning** practices staged keys and shows the layout reference during the first three stages. **Practice words** unlocks at stage 20 and uses randomly selected words from that layout's comma-delimited dictionary, without a keyboard reference. Both modes give immediate green/red feedback. Press `Enter` to submit a target; Backspace edits and `Esc` ends early. From the menu, `s` opens stats, `x` resets current-layout progress after confirmation, and `q` quits.

Every layout descriptor has exactly 50 stages. New letters are introduced gradually, then number and symbol keys; final stages can reinforce keys. A stage unlocks after at least 12 attempts and 85% accuracy on its introduced keys. Missed keys receive additional weight in Learning prompts. Stage 20 introduces words and unlocks Practice words. Dictionary words are filtered to characters currently unlocked.

## Custom practice words

Each descriptor names its own comma-delimited dictionary. Edit that file to add words. Colemak, Colemak-DH, Dvorak, Workman, and Russian Phonetic each have a separate CSV in `layouts/`. Words with characters not yet unlocked are skipped. Dictionaries are UTF-8 and support Cyrillic.

## Add a layout without changing the program

1. Create a UTF-8 text file named `layouts/<id>.layout` (or set `ALT_LAYOUTS_DATA_DIR` to a directory containing a `layouts` folder).
2. Add `id=<lowercase-id>` and `name=<display name>`.
3. Add one `row=<keys separated by spaces>` for each keyboard row. These rows drive the reference display; characters are used as supplied, including Cyrillic.
4. Add a `stages=` value with pipe-separated key groups. Group 1 is the starting group; subsequent groups introduce keys, one or more per group. Unicode/Cyrillic characters are supported. The app expands the progression to exactly 50 stages by repeating the last group. To customize all 50 stage contents instead, use exactly 50 repeated `stage=<keys>` lines and omit `stages=`.
5. Add `words=<filename.csv>` and create that comma-separated UTF-8 word file next to the descriptor. Words should be lowercase and contain only letters; words outside the currently unlocked key set are automatically excluded.

Example descriptor shape:

```text
id=my-layout
name=My Layout
row=q w e r t y u i o p
row=a s d f g h j k l
row=z x c v b n m
words=my-words.csv
stages=asdfjkl|q|w|e|r|t|y|u|i|o|p|z|x|c|v|b|n|m
```

The app scans `.layout` files at startup, sorts valid layouts by display name, and shows validation errors for invalid files. IDs must be unique and use lowercase ASCII letters, digits, and hyphens. The main menu's `l` option cycles through the discovered list. No Rust source changes or recompilation are needed to add a layout.

## Test

```sh
cargo test
cargo fmt --check
```

## Signed release packages

Pushing a version tag such as `v1.0.0` to GitHub runs the workflow in `.github/workflows/release.yml`. It builds Windows, macOS, and Linux packages containing the executable and layout data, creates a SHA-256 checksum file, and signs that checksum file using GitHub Actions OIDC with Sigstore keyless signing. The Sigstore bundle lets recipients verify the checksum/signature provenance; it does not claim an Authenticode/macOS code signature on the executable itself.

To use this workflow, put the project in a GitHub repository, push the default branch, then push a version tag (`git tag v1.0.0 && git push origin v1.0.0`). The workflow creates the GitHub release and attaches packages plus checksums and the Sigstore bundle. No long-lived signing key is stored in the repository.

The app uses Crossterm for portable terminal input. The Workman descriptor uses `qdrwbjfup; / ashtg yneoi / zxmcvkl,./`. Russian Phonetic uses a ChromeOS-style phonetic Cyrillic mapping as one example; phonetic layouts vary between operating systems, so set your OS to the matching variant for physical-key training.