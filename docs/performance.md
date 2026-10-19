# Performance

## Measuring

All benchmarks share one deterministic corpus (`src/bench/corpus.rs`) of
what programs actually send a terminal; `poopterm --benchmark --list` shows
it: build logs, colored compiler output with hyperlinks, `ls --color`,
highlighted source, accented/Greek/Cyrillic prose, CJK, emoji with ZWJ
sequences and combining marks, full-screen TUI redraws in synchronized
output, editor scrolling with scroll regions, progress bars, truecolor
half-block art, cursor motion, shell prompts with OSC 133/7/0/8, Sixel, and
kitty graphics (raw RGBA, PNG, zlib). Results are per workload plus an
overall figure (total bytes over total time), so a regression in one use
case cannot hide behind another.

**Headless**, parser and terminal, then with a frame rendered every MiB:

```sh
cargo build --release
./target/release/poopterm --benchmark                      # 16 MiB per workload, 120×40
./target/release/poopterm --benchmark --mib 64 --only log,tui --size 200x50
```

**End to end**, in any terminal (also for comparing emulators):

```sh
python3 scripts/bench-throughput.py                        # 16 MiB × 3 runs per workload
python3 scripts/bench-throughput.py --mib 64 --repeat 5 --only log --only emoji
python3 scripts/bench-throughput.py --format json --output results.json
```

It takes the workloads from `poopterm --bench-corpus`, generated for the
window's size, so every terminal gets identical bytes. Each timer stops only
after a DA1 (`CSI c`) round-trip proves the terminal parsed everything. It
reports the median and min–max of the runs plus the window size; compare
terminals at the same window size, since grid size affects the cost.

The terminal is named by its XTVERSION reply (`CSI > q`), not `TERM_PROGRAM`,
which a terminal launched from another one inherits. Every write, including
the warm-up, ends on a whole escape sequence and UTF-8 character: a DA1
query that lands inside an unfinished `CSI` is answered by poopterm and
xterm, but dropped by kitty, which used to abort the run.

**Parser only**, without rendering: `cargo test --release -- --ignored
--nocapture parser_throughput`. **PTY ceiling**: `scripts/bench-pty.py`.

To profile, build the `profiling` profile (release plus symbols) and use
`perf` (user-space events work without root):

```sh
cargo build --profile profiling
perf record -e cycles:u -- ./target/profiling/poopterm --benchmark --only emoji
perf report --sort sym
```

Where `perf` is unavailable, callgrind works on the same build (slower, but
exact instruction counts):

```sh
valgrind --tool=callgrind ./target/profiling/poopterm --benchmark --only kitty-png --mib 4 --no-render
callgrind_annotate --inclusive=yes callgrind.out.<pid> | head -40
```

The profile includes generating the corpus; look at the costs under
`bench::measure`.

### Profile-guided optimization

`poopterm --benchmark` needs no window and covers every workload, the
parser, terminal state, and rendering, so it is the PGO training run. With
[`cargo-pgo`](https://github.com/Kobzol/cargo-pgo):

```sh
cargo pgo build
./target/x86_64-unknown-linux-gnu/release/poopterm --benchmark --mib 8
cargo pgo optimize
```

Train on the whole corpus, not one workload, or the profile will favor it.
Compare the optimized build against a plain release build with the same
`--benchmark` arguments.

## Results

Intel i3-N305 (8 E-cores, AVX2), Linux, X11 window at 100×30 cells.

| Payload | Before (0.43.0) | After | Main change |
|---------|---------------:|------:|-------------|
| ASCII | 0.3 MB/s | ~100 MB/s | pipeline, pacing, row grid, SIMD fast path |
| Unicode | 0.3 MB/s | ~65 MB/s | pipeline, pacing, row grid, 16-byte cells |
| CSI-heavy | 0.3 MB/s | ~112 MB/s | pipeline, pacing |
| Cursor movement | 0.3 MB/s | ~58 MB/s | pipeline, pacing |

The ASCII figure is the median of 64 MiB runs; 16 MiB runs read around 87 MB/s.

Parser alone (200×50 grid): ASCII 4.4 → 131 MB/s, Unicode 3.0 → 57 MB/s,
CSI-heavy 59.5 → 110 MB/s.

## What changed, and why

| Problem | Fix | Where |
|---------|-----|-------|
| The reader blocked on the UI thread for every 16 KiB read, and nearly every read triggered a full-window render. | Shared output queue with one wake-up per batch and backpressure at 1 MiB; redraws paced to the display. | `pty/queue.rs`, `app/frame_pacer.rs` |
| Each line feed memmoved the whole screen. | Row-based `Grid`; scrolling rotates rows and moves scrolled-off rows into the scrollback. | `term/grid.rs` |
| Cells were 52 bytes, because an inline `[char; 8]` was reserved for combining marks. | 16-byte cells; marks and hyperlinks interned in a side table. | `term/extras.rs` |
| Every printed character re-derived its style and checked clusters and wide characters. | ASCII runs printed with one cell template per row segment. | `parser/print.rs` |
| vte dispatches one callback per character. | SIMD ground-state fast path; batching on the vte path. Later replaced by the in-house tokenizer. | `src/vt` |
| Every erase repaired wide characters on every row. | Repair only the touched row. | `parser/csi.rs` |
| Each drawn cell did two SipHash lookups plus a fontdue lookup. | One FxHash lookup per cell. | `render/glyphs.rs` |
| `config.toml` was read from disk on every key press. | Shortcuts parsed once. | `input/shortcuts.rs` |

Measured and rejected: batching scrollback evictions through a row pool,
because it showed no reproducible end-to-end gain and slowed the parser
benchmark.

## Threads, damage tracking and floods (26.10)

Parsing moved to a thread per tab and rendering became damage-tracked (see
[`architecture.md`](architecture.md)). Findings worth knowing before changing
either:

- **Throughput was not the win; responsiveness was.** Before, the UI thread
  parsed, and during a flood it barely rendered: on the Unicode payload it
  drew 2 frames in the last second, with a 1.06 s freeze. Now it draws 29
  (max gap 39 ms) at the same throughput.
- **Full frames compete with parsing even on other cores.** A HiDPI window
  (2232×1432) is 12.8 MB per frame; drawing it at 60 fps during a flood cut
  Unicode throughput from 66 to 55 MB/s. The parser executed the same
  instructions but at lower IPC; pinning threads to separate L2 clusters made
  no difference, which points at the shared L3 and memory bandwidth. Hence
  floods render at 30 fps (`app::FramePacer::set_flooding`): measured at
  16/33/100 ms frame intervals, Unicode went 55/64/67 MB/s.
- **Fills into softbuffer's buffer were slow.** ~75% of frame time was
  plain fills straight into the X11 shared-memory segment. Rendering into a
  persistent back buffer and copying only changed rows avoids that.

32 MiB ×3 medians, 100×30, same session: previous build vs 26.10.

| Payload | Before | 26.10 |
|---------|-------:|------:|
| ASCII | 97.5 | 94.8 |
| Unicode | 66.7 | 64.8 |
| CSI-heavy | 108.7 | 106.6 |
| Cursor movement | 56.6 | 55.4 |

These differences are within run-to-run noise (the baseline's ASCII figure
ranged 89–97 MB/s over the day).

## Where the time goes now

During a flood the parser thread is the bottleneck: it is ~100% busy and the
queue stays near full. On Unicode output, `Term::print_text` (width lookup
and cell writes) is ~60% of parser time and UTF-8 validation ~16%; on ASCII
output, line feeds (`scroll_region_up`: scrollback `Arc` allocation and row
blanking) take ~20%.
The plan toward 200 MB/s is in [`TODO.md`](../TODO.md).

## Follow-up work (26.10)

The TODO throughput work was implemented and validated with the parser and
terminal equivalence tests: UTF-8 decoding and width handling for common
ranges, scrollback row reuse, the empty-image fast path, and bounded parsing
for common CSI sequences. Poopterm preserves incomplete UTF-8 input across
feeds, including when a multibyte character is split between reads.

The PTY producer ceiling was measured on an Apple M4 with the discard-reader
benchmark in `scripts/bench-pty.py` (64 MiB ×3): ASCII 320, Unicode 323,
CSI-heavy 325, and cursor movement 305 MB/s. This confirms that the producer
path can exceed the 200 MB/s end-to-end goal on this host.

An owned-buffer queue prototype varied by payload in a 64 MiB ×5 comparison
(ASCII 213→266 MB/s, Unicode 242→225, CSI 189→194, cursor 162→165). A second
paired run did not complete, so the optimization was removed instead of
claiming an unproven gain. Selection and search row damage is checked by
pixel-equivalence tests; an ignored 300-frame test measured 25.6 ms for full
redraw and 3.39 ms for partial redraw at 100×30 cells and a 1000×600 surface.

## Images (26.10)

Headless `--benchmark` on the i3-N305, 16 MiB per workload, 120×40, parser
and terminal (with a frame rendered every MiB in parentheses), in MB/s of
encoded input:

| Workload | Before the kitty protocol | Now |
|----------|-------------------------:|----:|
| kitty-rgba | ignored | ~1110 (~700) |
| kitty-zlib | ignored | ~240 (~210) |
| kitty-png | ignored | ~115 (~107) |

Before, APC strings were dropped unread, so those workloads only measured
the scan for their end. Decoding base64 with SIMD took kitty-rgba from about
954 to 1113 MB/s. Reserving the exact size of raw payloads up front showed no
reproducible gain and was dropped.

PNG time is mostly the PNG crate: about 60% inflating (`fdeflate`) and 20%
unfiltering. Our own inflate (see [`TODO.md`](../TODO.md)) is where the next
gain is.
