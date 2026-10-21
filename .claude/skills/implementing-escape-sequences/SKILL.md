---
name: implementing-escape-sequences
description: Use when adding, fixing or changing how Poopterm handles a control sequence (CSI, ESC, OSC, DCS, APC, SGR, DEC private modes, queries and replies such as DA, DECRQM, XTVERSION), or when a program renders wrong in Poopterm.
---

# Implementing escape sequences

Sequences come from arbitrary programs: implement them completely, as the
specification defines them, with explicit bounds, and ignore what we do not
support without echoing it.

## 1. Read the specification first

xterm ctlseqs, the kitty protocol docs, iTerm2 docs, OSC 133, OSC 7501, and
so on. Note every parameter, its default, the query form and the exact reply
bytes. Cite the section in the pull request. Never read another terminal's
source (clean room).

## 2. Where the code goes

| Family | File |
|--------|------|
| Tokenizing (states, SIMD scans) | `src/vt/` — rarely needs changing |
| CSI | `src/parser/csi.rs` (`apply_csi_dispatch`), modes in `parser/modes.rs`, erase in `parser/erase.rs` |
| SGR | `src/parser/sgr.rs` |
| ESC | `src/parser/esc.rs` |
| OSC | `src/parser/osc.rs` (`apply_osc_dispatch`) routing to `osc_*.rs`; add a new `osc_<topic>.rs` rather than growing one |
| DCS | `src/parser/dcs.rs`, state in `src/term/dcs.rs` |
| Kitty graphics (APC `G`) | parsing in `src/graphics/kitty/`, applying in `src/term/kitty*.rs` |
| Window operations (`CSI t`) | `src/parser/window_ops.rs` |

State belongs in `Term` (`src/term/`). Replies go in `self.responses`; the
parser thread writes them to the PTY.

## 3. Test first, bytes in, state out

In the matching `src/term/*_tests.rs` (or `tests.rs`), with its helpers:

```rust
#[test]
fn decrqm_reports_bracketed_paste_as_set() {
	let mut t = terminal(10, 2);
	feed(&mut t, b"\x1b[?2004h\x1b[?2004$p");
	assert_eq!(t.responses, vec![b"\x1b[?2004;1$y".to_vec()]);
}
```

Cover: defaults (missing and `0` parameters), every parameter value, the
query and its reply, out-of-range and huge values, the sequence split
across reads, and that unknown variants change nothing and reply nothing.

## 4. Keep the oracles green and extended

- Add the new sequence to `PIECES` in `src/vt/tests.rs`: our tokenizer must
  produce the same events as vte for any stream.
- If it changes printing or grid state, add it to the fragments of
  `batched_printing_matches_per_character_printing` in `src/term/tests.rs`.

## 5. Bounds and privacy

- Every length, count and size from the program gets a named `MAX_*`
  constant (see `osc_clipboard.rs`, `osc_status.rs`), tested at the limit
  and one past it.
- Never answer a query that leaks data: clipboard reads (OSC 52 `?`),
  file contents, environment. Ignoring is the answer.
- Strip control characters from anything shown in the UI (titles, tab
  status, notifications).

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Implementing the set but not the query | Implement both; programs probe before they use. |
| Replying to an unsupported request with an error string | Stay silent. |
| Treating `0` and a missing parameter differently when the spec does not | Check the spec's default for each. |
| Allocating from a length the program sent | Cap it first. |
| Adding the terminfo capability but not the behavior (or the reverse) | Check `terminfo/` and `src/terminfo_caps.rs` together. |
