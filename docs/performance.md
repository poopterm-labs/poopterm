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

**Headless**, parser and terminal, then with a frame rendered every MiB.
Each run starts with a full scrollback, as after real use, so costs that grow
with history show up:

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

`scripts/pgo.sh` (or the `make pgo*` targets) builds an instrumented
binary, trains it, merges the profiles with the toolchain's own
`llvm-profdata` (from the `llvm-tools` component, added if missing) and
rebuilds with them. No other tools are needed.

```sh
make pgo             # instrument, train on the benchmark corpus, build optimized
make pgo-use         # dogfood the instrumented terminal; closing it adds a profile
make pgo-build       # rebuild from every profile collected so far
make pgo-compare     # release vs PGO, same benchmark
make pgo-install     # install the optimized binary system-wide
make pgo-clean       # forget the profiles
```

`poopterm --benchmark` needs no window and covers every workload, the
parser, terminal state and rendering, so it is the base training run
(`PGO_MIB`, default 8, sets its size). Sessions with `make pgo-use` add the
paths only a window exercises: input, tabs, the UI and real programs.
Profiles accumulate under `target/pgo/profiles` until `make pgo-clean`;
after large code changes, clean and retrain. On an i3-N305 the
benchmark-trained build is about 6% faster overall (up to 17% on shell
prompts, 10% on emoji).

## Results

Intel i3-N305 (8 E-cores, AVX2), Linux, 26.10.3 plus the synchronized-output
and combining-mark changes below (`d48ebd6`), plain release build (no PGO).
**End to end** is `scripts/bench-throughput.py --mib 16 --repeat 3` in a
poopterm window of 100×30 cells (median MB/s); **headless** is
`poopterm --benchmark --mib 16 --no-render` at 120×40, starting from a full
scrollback.

| Workload | End to end | Headless |
|----------|-----------:|---------:|
| log (plain ASCII) | 86 | 204 |
| compiler (colors, hyperlinks) | 77 | 122 |
| ls --color | 86 | 143 |
| source (highlighted) | 105 | 131 |
| unicode (accented, Greek, Cyrillic) | 83 | 92 |
| cjk | 86 | 102 |
| emoji (ZWJ, combining marks) | 63 | 69 |
| tui (sync blocks, htop-like) | 161 | 175 |
| editor (scroll regions) | 159 | 165 |
| progress bars | 175 | 190 |
| truecolor half blocks | 248 | 261 |
| cursor motion | 119 | 123 |
| shell prompts (OSC 133/7/0/8) | 90 | 115 |
| sixel | 72 | 76 |
| kitty-rgba | 301 | 1021 |
| kitty-png | 108 | 111 |
| kitty-zlib | 224 | 233 |
| **overall** | **108** | **133** |

End-to-end figures sit below headless ones where the PTY and the window
cost more than parsing: plain ASCII parses at 200 MB/s but arrives at
86 MB/s, and raw kitty RGBA is limited by the PTY, not the decoder. Where
parsing dominates (emoji, Unicode, tui) the two are close.

### Before 26.10

The first round of work, 0.43.0 to 26.10.0, on an X11 window at 100×30:

| Payload | 0.43.0 | 26.10.0 | Main change |
|---------|-------:|--------:|-------------|
| ASCII | 0.3 MB/s | ~100 MB/s | pipeline, pacing, row grid, SIMD fast path |
| Unicode | 0.3 MB/s | ~65 MB/s | pipeline, pacing, row grid, 16-byte cells |
| CSI-heavy | 0.3 MB/s | ~112 MB/s | pipeline, pacing |
| Cursor movement | 0.3 MB/s | ~58 MB/s | pipeline, pacing |

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

## Synchronized output and combining marks (26.10)

- **History-dependent costs were invisible.** Each headless run used a
  fresh terminal, so tui read 188 MB/s while a real terminal with a full
  scrollback managed 30. Every `?2026` block cloned the whole `Term`,
  scrollback and image stores included. Runs now start from a full
  scrollback, and a block freezes one `Frame` of the visible screen instead
  (`term/sync.rs`): tui went from 32 to about 180 MB/s headless.
- **Interning combining marks** hashed a freshly built entry with SipHash
  for every mark. Hashing in one write and looking up once (`term/extras.rs`)
  recovered a 10% regression; a 256-slot cache of `(id, mark) → id` in front
  of the table took emoji from 43 to 69 MB/s headless. SipHash stays,
  because programs choose the hashed content.
- **Titles** set by prompts on every line were filtered character by
  character; short printable-ASCII titles are now copied whole (shell +7%).
- **Profile-guided optimization** (`make pgo`) adds about 6% overall on top
  of all this.

## Where the time goes now

During a flood the parser thread is the bottleneck: it is ~100% busy and the
queue stays near full. On emoji output, `Term::print_text` is ~22% of
parser time, UTF-8 validation ~9%, line feeds (`scroll_region_up`) ~8% and
attaching marks ~7%; the interning that used to dominate is mostly gone. On
Unicode output, `print_text` (width lookup and cell writes) dominates; on
ASCII output, line feeds take ~20%.
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
