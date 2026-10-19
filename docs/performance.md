# Performance

## Measuring

Two benchmarks, both deterministic:

**End to end**, in any terminal (also useful for comparing emulators):

```sh
python3 scripts/bench-throughput.py                 # 32 MiB × 3 runs per payload
python3 scripts/bench-throughput.py --mib 64 --repeat 5 --only ascii
python3 scripts/bench-throughput.py --format json --output results.json
```

It writes ASCII, Unicode, SGR-heavy and cursor-movement payloads, and stops
each timer only after a DA1 (`CSI c`) round-trip proves the terminal parsed
everything. It reports the median and min–max of the runs plus the window
size; compare terminals at the same window size, since grid size affects
the cost.

**Parser only**, without the PTY or rendering:

```sh
cargo test --release -- --ignored --nocapture parser_throughput
```

To profile, build with symbols and use `perf` (user-space events work
without root):

```sh
CARGO_PROFILE_RELEASE_DEBUG=true CARGO_PROFILE_RELEASE_STRIP=false cargo build --release
perf record -e cycles:u -p "$(pgrep -x poopterm)" -- sleep 5
perf report --sort sym
```

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
| vte dispatches one callback per character. | SIMD ground-state fast path; batching on the vte path. | `parser/mod.rs`, `parser/scan.rs` |
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
queue stays near full. On Unicode output, vte's `ground_dispatch` plus UTF-8
validation take ~55% of parser time; on ASCII output, line feeds
(`scroll_region_up`: scrollback `Arc` allocation and row blanking) take ~20%.
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
