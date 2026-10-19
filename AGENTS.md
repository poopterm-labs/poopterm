# AGENTS.md

Instructions for AI coding agents (any model or tool) and for humans working
on Poopterm. This file is the single source of truth; `CLAUDE.md` and any
other agent-specific files only point here.

Poopterm is a software-rendered terminal emulator written in Rust: winit for
windows, softbuffer for pixels, fontdue for glyphs, portable-pty for shells,
and its own SIMD escape-sequence tokenizer (`src/vt`), tested against vte.

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
  on Linux, macOS, and a FreeBSD VM. All must pass.
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
PTY reader thread ──► pty::OutputQueue ──► parser thread (pty::pump) ──► term::SharedTerm
                        (backpressure)        parser::Parser (vt + SIMD)      (Mutex<Term>)
                                                   │ wake once per batch          │
                                                   ▼                              │ snapshot
input events ──► App (UI thread) ──► input::{keys,mouse,paste} ──► pty::PtyWriter ──► shell
                     │                                                            │
                     └─ app::FramePacer ──► app::draw ◄── term::Frame ◄───────────┘
                          (30 fps in floods)    └─ render::damage + app::canvas ──► softbuffer
```

| Path | Responsibility |
|------|----------------|
| `src/term/` | Terminal state: `Grid` (one `Vec<Cell>` per row), scrollback, cursor, modes, Sixel placement, synchronized output. `Cell` is 16 bytes; combining marks and hyperlinks live in `term::extras`. `SharedTerm` is the lock shared with the parser thread; `Frame` is the copy the renderer draws. |
| `src/vt/` | Escape-sequence tokenizer: the DEC state machine with SIMD scans, handing text, CSI/ESC/OSC/DCS/APC events to a `vt::Handler`. vte is only a test oracle. |
| `src/parser/` | Applies `vt` events to `Term` (`impl Handler for Term`); `csi`, `esc`, `osc`, `sgr`, `print` apply each family. |
| `src/pty/` | Shell process, reader and parser threads (`pump`), `OutputQueue`, non-blocking `PtyWriter`. |
| `src/input/` | Pure encoders: keys, mouse reports, paste, scroll, shortcuts. |
| `src/graphics/` | Image data shared by Sixel and the kitty graphics protocol: `Bitmap`, `Placement`, streaming base64. `graphics::kitty` parses commands, receives chunked transfers, decodes and stores images and animation frames; `term/kitty*.rs` applies commands to `Term`. |
| `src/render/` | Pure drawing: `Painter`, `FontSet` glyph cache, grid renderer, row damage, SIMD blending. |
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
  the parser thread does the reading and parsing.
- **Hold the terminal lock briefly, and never twice.** `Session::term()`
  returns a guard; drop it before calling anything that may lock again
  (debug builds panic instead of deadlocking). Watch for temporaries:
  `let x = a(self.active_term().y) && self.b()` keeps the guard alive for
  the whole statement. Render from a `term::Frame`, never under the lock.
- **The tokenizer must agree with vte.** Any change to `src/vt` must keep
  the differential tests in `vt/tests.rs` green, and any change to
  `Term::print_ascii` / `print_text` must keep
  `batched_printing_matches_per_character_printing` green; extend their
  fragment lists when you add new cases.
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
  requests.
- **Messages follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/):**
  `type(scope)!: summary`, with the scope optional, and the summary in lower
  case, imperative mood, and without a final period. Types: `feat`, `fix`,
  `perf`, `refactor`, `docs`, `test`, `bench`, `build`, `ci`, `style`,
  `chore`, `revert`. Scopes name the area touched, usually a module
  (`parser`, `vt`, `kitty`, `render`, `pty`, `input`, `bench`, `release`).
  Breaking changes get `!` and a `BREAKING CHANGE:` footer. The body explains
  *why*. Add no session links or other tool trailers; a `Co-Authored-By:`
  footer is fine.
- **Versioning:** `YEAR.MONTH.PATCH` (two-digit year, month without a leading zero, e.g. 26.1.0 for January), plus a random
  codename per release (26.10.0 is "Giga Fly"). Put the codename in
  `[package.metadata.poopterm] codename` in `Cargo.toml` and in the
  changelog heading (`## 26.10.0 "Giga Fly" — date`); CI uses it in the
  release title. Patch releases keep their release's codename.
- After fully green CI on `main`, CI increments the patch or starts a new
  month at `YY.M.0` with a new random codename. It commits `Cargo.toml`,
  `Cargo.lock`, and `CHANGELOG.md` together, atomically pushes the commit and
  tag only if `main` still matches the checked revision, publishes the release,
  and then updates `Formula/poopterm.rb`. `GITHUB_TOKEN` pushes do not retrigger
  CI. Do not bump versions manually for normal work; `git pull --rebase`
  before pushing. Release logic lives in `scripts/release.py` and
  `scripts/release.sh`; test it with `python3 -m unittest discover -s tests`.

## Verifying UI changes

Unit tests cannot see pixels. For rendering changes, run the app
(`cargo run --release`) and look, or compare screenshots of a fixed screen
before and after, and partial redraws against `POOPTERM_FULL_REDRAW=1`; the process is described in
[`docs/testing.md`](docs/testing.md#visual-checks).
