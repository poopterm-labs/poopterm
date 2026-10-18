# TODO

Open work, roughly in priority order. Measure before and after every step
(`docs/performance.md`); drop changes without a reproducible gain.

## Throughput: 200 MB/s end to end

Current (26.10.0, i3-N305, 100×30, `scripts/bench-throughput.py`, 32 MiB ×3):
ASCII ~95, Unicode ~65, CSI-heavy ~107, cursor movement ~56 MB/s. The
parser thread is the bottleneck: during a flood it is ~100% busy and the
1 MiB queue stays near full. End to end at 200 MB/s needs the parser alone
well above 250 MB/s.

0. **Find the ceiling first.** Measure how fast the benchmark's producer can
   push bytes through a PTY with a trivial consumer (a reader thread that
   discards). If the kernel PTY path plus Python caps near 200 MB/s on this
   machine, the target must be measured on faster hardware or with a
   C/Rust producer.
1. **UTF-8 without vte.** On the Unicode payload, vte's `ground_dispatch`
   plus `core::str::from_utf8` take ~55% of parser time. Extend the SIMD
   ground fast path (`parser/scan.rs`, `parser/mod.rs`) to validate and decode
   UTF-8 runs itself, and print them through a batched path like
   `Term::print_ascii`, with a width table for common ranges (a CJK/emoji
   fast check, `unicode-width` only for the rest). Keep
   `batched_printing_matches_per_character_printing` green and add UTF-8
   fragments (split sequences, invalid bytes, combining marks, wide chars at
   the right margin).
2. **Cheaper line feeds.** `Term::scroll_region_up` is ~20% on ASCII:
   every scrolled line allocates an `Arc` for the scrollback and blanks a
   recycled row. Options: store scrollback rows without `Arc` (a ring of
   `Vec<Cell>`), and blank lazily (track each row's used length instead of
   filling it with blanks).
3. **Skip image bookkeeping without images.** `Term::erase_images` runs on
   every scroll (~3%); return early when there are no Sixel placements.
4. **In-tree CSI for common sequences.** SGR and cursor movement dominate
   the CSI and cursor payloads. Parse the most common forms (`CSI m`,
   `CSI H`, `CSI K`, `CSI A–D`) in the fast path; hand anything else to vte.
5. **Fewer copies on the way in.** The reader copies each read into the
   queue. Reading directly into the queue's spare capacity saves a memcpy
   per byte (~1–2%).

## Rendering

- During floods the UI renders full frames at 30 fps. Each HiDPI frame is
  12.8 MB of pixels that competes with parsing for the shared L3 cache.
  Ideas: skip unchanged background pixels in full frames, or render floods
  at a lower internal resolution.
- Selection and search highlights force full frames; damage only the rows
  whose highlight changed.

## Other

- Report the vte 0.15 split-UTF-8 byte drop upstream (worked around in
  `parser::Parser::advance`).
- Update `Formula/poopterm.rb` to v26.10.0 once CI has tagged it.
- Bash shell integration still needs manual setup; consider injecting it
  via `--rcfile` like zsh's `ZDOTDIR` loader.

## CI and platforms (requested, not started)

- **Automatic monthly releases:** after every fully green CI run on `main`,
  CI bumps the patch (`YY.M.P+1`), or starts `YY.M.0` with a new random
  codename in a new month. It commits the bump to Cargo.toml, Cargo.lock and
  the CHANGELOG with `GITHUB_TOKEN` (those pushes don't retrigger CI), tags
  it and creates the release. It skips the release if `main` moved during the
  run. Then it updates `Formula/poopterm.rb`. Contributors must
  `git pull --rebase` before pushing.
- **FreeBSD CI:** build and test in a VM (e.g. `vmactions/freebsd-vm`) with
  the xorg, libxkbcommon and wayland packages; today only cross-clippy runs.
- **Android:** `rfd` does not compile for Android (make it a non-Android
  dependency). Also needed: an `android_main` entry through winit's
  `android-native-activity`, clipboard/file-dialog/cwd fallbacks,
  `/system/bin/sh` and `/system/fonts`, an APK build in CI (cargo-apk or
  xbuild), and testing on a device or emulator.
