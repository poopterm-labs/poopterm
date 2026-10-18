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

## Where the time goes now

A profile of the ASCII payload, end to end: blanking each new bottom row and
pushing the scrolled-off row into the scrollback (~35%), writing cells
(~25%), rendering at 60 fps (~17%).

Options for going further, roughly by payoff:

1. **Parse on a dedicated thread.** Rendering would no longer take time from
   parsing; this needs a lock or snapshot handoff between the parser and the
   renderer.
2. **Damage-tracked rendering.** Redraw only changed rows. This cuts render
   cost during floods and CPU use while typing; overlays, the cursor and
   selection must invalidate correctly.
3. **Lazy row blanking.** Track how much of each row is in use instead of
   clearing every recycled row. Requires changing how rows are indexed.
4. **Adaptive frame rate during floods**, e.g. 30 fps while output exceeds
   a threshold. Trivial, but scrolling looks choppier during floods.
5. **Replace vte** with an in-tree parser so the SIMD path also covers
   UTF-8 text and common CSI sequences. Large effort; mostly helps Unicode
   and CSI-heavy output.
