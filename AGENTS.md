# AGENTS.md

Instructions for AI coding agents (any model or tool) and for humans working
on Poopterm. This file is the single source of truth; `CLAUDE.md` and any
other agent-specific files only point here.

Poopterm is a software-rendered terminal emulator written in Rust: winit for
windows, softbuffer for pixels, fontdue for glyphs, portable-pty for shells,
and vte for tokenizing escape sequences.

## Quick reference

```sh
cargo test                              # unit + emulation tests (fast, no window)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release                   # binary: target/release/poopterm
cargo test --release -- --ignored --nocapture parser_throughput   # parser benchmark
```

- Toolchain: **nightly** (pinned by `rust-toolchain.toml`) for `portable_simd`.
- Formatting: hard tabs, width 100 (`rustfmt.toml`). Always run `cargo fmt`.
- Linux build deps: `pkg-config libx11-dev libxkbcommon-dev libwayland-dev`.
- CI (`.github/workflows/ci.yml`) runs fmt, test, clippy and a release build
  on Linux and macOS (plus a FreeBSD clippy check). All must pass.
- **Platform-gated code** (`#[cfg(target_os = ...)]`) is not compiled on your
  host. Before committing such changes, check the other targets too:
  `rustup target add aarch64-apple-darwin x86_64-unknown-freebsd`, then
  `cargo clippy --target <triple> --all-targets -- -D warnings` for each.

## Workflow: test-driven development

Every behavior change and bug fix follows red → green → refactor:

1. **Red.** Write a test that describes the desired behavior. Run it and
   confirm it fails *for the expected reason* (wrong output, or the API not
   existing yet), not because of a typo.
2. **Green.** Write the smallest change that makes it pass.
3. **Refactor.** Clean up while the whole suite stays green.
4. Run the full checks (test, clippy, fmt) before committing.

A test that passes the first time you run it proves nothing; adjust it until
you have seen it fail. Pure refactors keep existing tests green and add
characterization tests for code that had none. See
[`docs/testing.md`](docs/testing.md) for where tests live and how to write
emulation tests.

Performance work is measured, not guessed: benchmark before and after (see
[`docs/performance.md`](docs/performance.md)) and drop changes that do not
show a reproducible gain.

## Architecture in one screen

```
PTY reader thread ──► pty::OutputQueue ──► app::process_output ──► parser::Parser ──► term::Term
     (pty/mod.rs)       (backpressure)        (UI thread)           (vte + SIMD)       (grid state)
                                                   │
input events ──► input::{keys,mouse,paste} ──► pty::PtyWriter (writer thread) ──► shell
                                                   │
                         app::FramePacer ──► app::draw ──► render::{painter,grid} ──► softbuffer
```

| Path | Responsibility |
|------|----------------|
| `src/term/` | Terminal state: `Grid` (one `Vec<Cell>` per row), scrollback, cursor, modes, Sixel placement, synchronized output. `Cell` is 16 bytes; combining marks and hyperlinks live in `term::extras`. |
| `src/parser/` | Applies escape sequences to `Term`. `mod.rs` holds the ground-state SIMD fast path and the vte bridge; `csi`, `esc`, `osc`, `sgr`, `print` apply each family. |
| `src/pty/` | Shell process, reader thread, `OutputQueue`, non-blocking `PtyWriter`. |
| `src/input/` | Pure encoders: keys, mouse reports, paste, scroll, shortcuts. |
| `src/render/` | Pure drawing: `Painter`, `FontSet` glyph cache, grid renderer, SIMD blending. |
| `src/ui/` | Pure UI models: selection, search, tab layout. |
| `src/app/` | winit event loop and glue. `App` methods are split by concern (`keyboard`, `mouse`, `tabs`, `settings`, `draw`, …). |
| Top level | `config`, `theme`, `fonts`, `clipboard`, `session_store`, `shell_integration`, `sixel`, `wallpaper`, `platform`. |

Details: [`docs/architecture.md`](docs/architecture.md).

## Rules of thumb

- **Keep files small and single-purpose.** Prefer a new module over growing
  a file past roughly 400 lines. Each file starts with a `//!` line saying
  what it is for.
- **Keep logic pure and testable.** Put encoding, layout and state changes in
  functions that take plain data (see `input::keys::encode_key`,
  `ui::tabs::tab_layout`, `app::settings::settings_target`). `App` methods
  should mostly route events to those functions.
- **Never block the UI thread** on the PTY. Write through `Session::write`;
  read through the output queue.
- **The parser fast path must agree with vte.** Any change to
  `parser::Parser::feed`, `Batch`, or `Term::print_ascii` must keep
  `batched_printing_matches_per_character_printing` green; extend its
  fragment list when you add new cases.
- **Bounds on untrusted input.** Escape sequences come from arbitrary
  programs. Keep explicit limits (Sixel size, OSC 52 size, hyperlink count,
  titles) and never answer requests that leak data (OSC 52 reads are ignored
  on purpose).
- **Config compatibility.** `config.toml` keys are user-facing; keep old
  names working (see `ThemeKind::parse`) and preserve unknown keys on save.
- Match surrounding style: comment density, naming, and the existing `pub(crate)`
  / `pub(super)` visibility patterns. No new dependencies without a reason.

## Commits and releases

- Work lands directly on `main` as small, focused commits; there are no pull
  requests. Write messages that explain *why*, prefixed `fix:`, `perf:`,
  `feat:`, `refactor:`, `docs:`, `bench:` or `chore:`.
- CI on `main` creates a `vX.Y.Z` tag and GitHub release when `Cargo.toml`'s
  version has no tag yet. A version bump is therefore a release: update
  `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md` together, and update
  `Formula/poopterm.rb` only after the tag exists.

## Verifying UI changes

Unit tests cannot see pixels. For rendering changes, run the app
(`cargo run --release`) and look, or compare screenshots of a fixed screen
before and after; the process is described in
[`docs/testing.md`](docs/testing.md#visual-checks).
