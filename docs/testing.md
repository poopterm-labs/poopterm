# Testing

Poopterm is developed test-first (see the workflow in
[`AGENTS.md`](../AGENTS.md#workflow-test-driven-development)). This page
covers where tests live and how to write them.

## Running

```sh
cargo test                       # everything except benchmarks; no window needed
cargo test term::                # only terminal-emulation tests
cargo test -- --nocapture name   # one test with its output
cargo test --release -- --ignored --nocapture parser_throughput   # benchmark
```

Tests that need an external program (the shell-integration tests run `zsh`
and `bash`) return early when it is missing, so they pass on minimal CI
images and run fully on developer machines.

## Where tests go

Unit tests sit at the bottom of the file they test, in a `#[cfg(test)] mod
tests`. The exceptions are larger suites:

| Suite | What it covers |
|-------|----------------|
| `src/term/tests.rs` | Emulation: bytes in, grid/cursor/modes/replies out. Includes the randomized bounds test and the parser differential test. |
| `src/input/*` | Exact byte sequences for keys, mouse reports, paste and scrolling. |
| `src/app/settings.rs`, `src/ui/*` | Hit-testing and layout as pure functions. |
| `src/pty/*` | Output queue backpressure and the non-blocking writer (real pipes and threads). |
| `src/shell_integration.rs` | The shipped zsh/bash hooks, run in real shells. |

## Writing emulation tests

Use the helpers at the top of `src/term/tests.rs`:

```rust
#[test]
fn erase_line_keeps_the_current_background() {
	let mut t = terminal(4, 2);           // 4 columns × 2 rows
	feed(&mut t, b"abcd\x1b[44m\r\x1b[K");
	assert_eq!(row(&t, 0), "    ");       // text of row 0
	assert_eq!(cell(&t, 0).bg, t.palette[4]);
}
```

- `feed` runs bytes through the real `Parser`.
- `row(&t, y)` returns a row's characters; `cell(&t, i)` returns the cell at
  flat index `y * cols + x`.
- Terminal replies accumulate in `t.responses`.
- Name tests after the behavior (`no_wrap_overwrites_last_column`), not the
  function.

When fixing a parser bug, add the triggering sequence to the fragments in
`vt/tests.rs` (tokenizer events against vte) and, when it affects terminal
state, to `batched_printing_matches_per_character_printing`. A failing
`vt/tests.rs` case is shrunk to a minimal stream automatically.

## Pure functions over UI state

Code that needs a window is hard to test, so keep decisions in functions
that take plain data and return plain data: `encode_key`, `encode_mouse`,
`settings_target`, `tab_layout`, `FramePacer::request`. `App` methods then
only gather inputs and apply results.

## Visual checks

Rendering has no automated pixel tests. When changing drawing code, compare
a fixed screen before and after:

1. Write a fake shell that prints a deterministic sample and sleeps (styled
   text, 256 colors, wide and combining characters, box drawing).
2. Launch both builds with it, using a temporary config so your own settings
   and sessions are untouched:
   `POOPTERM_CONFIG=/tmp/cfg/config.toml SHELL=/path/fake-shell.sh target/release/poopterm`
3. Screenshot both windows (on X11, `import -window <id> out.png`) and compare
   with ImageMagick: `compare -metric AE before.png after.png diff.png`.
   Zero differing pixels means the change is visually neutral.

Notes:

- Let the fake shell `sleep 1` before printing. Output is parsed as soon as it
  arrives, so a shell that prints before the window has its final size lays
  text out at the initial 80 columns, and the screenshot depends on timing.
- **Damage tracking.** Most frames redraw only changed rows. To check partial
  redraws, make the fake shell print in steps (scrolling, cursor moves), screenshot the same build with and without
  `POOPTERM_FULL_REDRAW=1`, and compare. That variable disables partial
  redraws.
- Debug builds panic if a thread locks a terminal it already holds, so run
  new UI code paths in a debug build at least once.
