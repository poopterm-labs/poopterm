# Contributing to Poopterm

Thanks for helping. Poopterm is a small terminal emulator with big
ambitions for speed and correctness, so contributions are held to a few
firm rules. The full, authoritative rules live in [`AGENTS.md`](AGENTS.md),
shared by human contributors and coding agents alike; this page is the
short version and the practical steps.

By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
Security problems are not reported in public issues; see
[`SECURITY.md`](SECURITY.md).

## Getting set up

1. Install Rust with [rustup](https://rustup.rs). The repository pins a
   **nightly** toolchain in `rust-toolchain.toml` (needed for
   `portable_simd`); rustup picks it up automatically.
2. Install the platform build dependencies:
   - Linux: `pkg-config libx11-dev libxkbcommon-dev libwayland-dev`
     (Debian/Ubuntu names; see the README for Fedora and others).
   - FreeBSD: `pkg install rust pkgconf xorg libX11 libxkbcommon wayland`.
   - macOS: Xcode command line tools.
3. Build and run:

   ```sh
   cargo run --release
   ```

## Before you send a change

Every change must pass the same checks CI runs on Linux, macOS and FreeBSD:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 -m unittest discover -s tests   # release and benchmark scripts
```

If you touch `#[cfg(target_os = ...)]` code, also run clippy for the other
targets (`aarch64-apple-darwin`, `x86_64-unknown-freebsd`); see
[`AGENTS.md`](AGENTS.md#quick-reference).

## How we work

- **Test first.** Every behavior change and bug fix starts with a test that
  fails for the right reason, then the smallest change that makes it pass,
  then cleanup. [`docs/testing.md`](docs/testing.md) explains where tests live
  and how to write emulation tests.
- **Measure performance work.** Benchmark before and after with the tools in
  [`docs/performance.md`](docs/performance.md); changes without a reproducible
  gain are dropped.
- **Check rendering changes with your eyes.** Unit tests cannot see pixels.
  Compare screenshots before and after, and partial redraws against
  `POOPTERM_FULL_REDRAW=1`, as described in
  [`docs/testing.md`](docs/testing.md#visual-checks).
- **No new dependencies.** Features are written in-house from the
  specifications, clean room: never read or copy another implementation's
  source. Existing crates are being replaced over time (see
  [`TODO.md`](TODO.md)), each one tested differentially against the crate it
  replaces.
- **Treat terminal output as hostile.** Anything a program prints can be an
  attack. Keep explicit size limits, never answer queries that leak data,
  and ignore unknown sequences silently.
- **Keep files small and logic pure.** Prefer a new module over growing a
  file past about 400 lines; put encoding, layout and state changes in plain
  functions that tests can call without a window.

[`docs/architecture.md`](docs/architecture.md) is the map of the code.

## Commits

Commits follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/):

```
type(scope): summary in lower case, imperative, no final period

The body explains why the change is needed.
```

Types are `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `bench`, `build`,
`ci`, `style`, `chore` and `revert`; the scope names the area, usually a
module (`parser`, `vt`, `kitty`, `render`, `pty`, `input`). Keep each commit
small and about one thing: the changelog is generated from these summaries,
so they should read well there.

Do not bump versions or edit `CHANGELOG.md` by hand; CI does that after a
green build on `main`.

## Sending changes

Maintainers commit directly to `main` in small, focused commits. If you are
not a maintainer:

1. Open an issue first for anything larger than a small fix, so we can agree
   on the approach before you spend time on it.
2. Send a pull request with your commits. A maintainer reviews it and lands
   the commits on `main` (rebased, not merged), keeping your authorship.

## Reporting bugs

Open an issue with:

- your OS and version, display server (X11 or Wayland) and Poopterm version
  (Settings → About, or `poopterm --version`);
- what you did, what you expected, and what happened;
- for rendering or escape-sequence bugs, the exact bytes that trigger it
  (for example captured with `script`), and a screenshot if it is visual.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
