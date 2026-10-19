# TODO

Open work, roughly in priority order. Measure before and after every step
(`docs/performance.md`); drop changes without a reproducible gain.

## Throughput goals

Everything should be as fast as we can make it while staying stable and
correct. The numbers below are minimums, not finish lines.

**Text: at least 200 MB/s end to end on every payload** (ASCII, Unicode,
CSI-heavy, cursor movement). 200 MB/s is the floor, not the ceiling: go
higher wherever that stays safe (bounded memory, no skipped state, output
identical to the slower path). Measured with `scripts/bench-throughput.py` in a real
window. Latest i3-N305 numbers are 63–248 MB/s depending on the workload
(108 overall); emoji, compiler output, Unicode, ls and plain logs are
furthest below the floor. See [`docs/performance.md`](docs/performance.md).

**Images: as fast as possible** for Sixel and the kitty graphics protocol,
from first byte to pixels on screen. There is no target to stop at; at least
match text throughput, within the explicit size limits on untrusted input.

- **Image benchmark.** Sixel and kitty payloads (raw RGBA, PNG, zlib) are
  in the corpus, so `--benchmark` and `bench-throughput.py` cover them. Still
  missing: full-screen frames and animation, reported in frames per second.
- **Sixel decoding.** Profile `src/sixel.rs` against that benchmark; candidates
  are SIMD band decoding, decoding straight into the placement bitmap, and
  streaming decode instead of buffering the whole DCS.
- **Kitty image decoding.** Raw RGBA parses at ~1.1 GB/s; PNG at ~115 MB/s
  spends ~60% inflating and ~20% unfiltering inside the PNG crate. Our own
  inflate (below) plus a SIMD PNG unfilter are the next steps; the RGBA path
  could decode base64 straight into pixels instead of through a byte buffer.
- **Rendering during floods:** frames still write unchanged background pixels,
  competing with parsing for memory bandwidth. Evaluate skipping those pixels
  or lowering internal resolution, retaining a change only with repeatable
  throughput and visual gains.
- **Reduce copies in PTY input:** an owned-buffer queue prototype did not show
  a repeatable gain across payloads and was removed. Revisit only with a
  benchmark that isolates copy cost while preserving queue backpressure.
- **Fair comparisons.** Compare terminals at the same window size and in the
  same session; the benchmark now names terminals by XTVERSION and no longer
  aborts in kitty.

## Hardware-accelerated rendering

Full GPU rendering, now that kitty graphics has landed. CPU drawing is the main
latency cost: a full redraw takes 25.6 ms at 1000×600, and much longer at
HiDPI sizes. It also takes memory bandwidth from the parser during floods,
which is why floods are capped at 30 fps. Parse throughput stays CPU work.

- **Strict rule: no `wgpu`, WebGPU or other GPU abstraction crates.**
  Write our own renderers on raw bindings: Vulkan on Linux and FreeBSD,
  Metal on macOS. Use thin FFI bindings (like `ash`, `objc2-metal`), or
  write our own where that is practical.
- **Software rendering stays as the fallback**, used when the GPU path fails
  to start (VMs, X forwarding, broken or missing drivers). It is also the
  reference: GPU output must match it in the pixel-equivalence tests.
- **Design:** a small `Renderer` boundary (a `term::Frame` and damage go in,
  a presented frame comes out), so `term` and `parser` never see the
  backend. Expected pieces: a glyph atlas with partial uploads, instanced
  cell quads for backgrounds, cursor and selection, and image textures for
  Sixel and kitty placements (GPU scaling and blending). The 30 fps flood
  cap goes away if measurements show parsing no longer suffers.
- Measure frame time, input latency and flood throughput before and after
  on every platform, as for any performance work.

## Resource management and memory

- **cgroups v2 per tab, fully automatic (Linux).** Like Ghostty, put
  each shell in its own cgroup, so a runaway program in one tab cannot
  starve the terminal or the other tabs. The terminal process keeps its
  own group. No configuration: at startup, check that `/sys/fs/cgroup` is
  a cgroup2 mount (`cgroup.controllers` exists) and that we may create
  groups there. Do that either through a delegated subtree we can write
  (our own cgroup's directory) or by asking systemd for a transient user
  scope over D-Bus. If neither works, run exactly as today with no
  errors. Start with per-tab groups (memory and pids accounting, a
  `memory.high` so one tab is throttled before the OOM killer picks the
  terminal), then consider CPU weight so the UI and parser threads stay
  responsive while a tab floods. Test in CI: available, read-only and
  missing cgroupfs all behave correctly.
- **Transparent huge pages for large buffers.** Large, long-lived
  allocations (scrollback rows, image bitmaps, the frame buffer, PTY
  queues) could cause fewer TLB misses on huge pages. Check once at
  startup by reading `/sys/kernel/mm/transparent_hugepage/enabled`
  (`always` or `madvise`, cached). Then `madvise(MADV_HUGEPAGE)` only
  allocations of at least 2 MiB, aligned so they can actually use huge
  pages. FreeBSD promotes superpages by itself, and macOS has no
  equivalent, so it is Linux-only and does nothing elsewhere. Measure RSS
  and throughput before and after, and keep it only for buffers where it
  helps reproducibly.

## Replacing dependencies with our own code

Chosen to replace with our own implementations, built around this
project's needs and optimizations. Each replacement is tested against the
crate it replaces as an oracle (differential tests, and fuzzing for parsers
of untrusted input), and lands only once it matches it and measures at
least as fast.

- **`portable-pty` → our own PTY layer** (libc: `posix_openpt`/`openpty`,
  `fork` + `setsid` + `TIOCSCTTY`, `exec`, `TIOCSWINSZ`, non-blocking fds).
  It gives us control of read sizes and polling, and drops
  Windows-oriented abstractions we do not use. Must pass CI on Linux, macOS
  and FreeBSD.
- **`unicode-width` → generated width tables:** a build-time generator from
  the Unicode data files, emitting a two-level lookup table, combined with
  the fast ranges in `parser/utf8.rs`. An exhaustive test checks every
  code point against the crate. Regenerate it for each Unicode release.
- **`miniz_oxide` → our own inflate (RFC 1950/1951)** for kitty `o=z` and
  PNG. **Safety and security come first:** this parses untrusted input, so
  no `unsafe` unless it is proven necessary and justified,
  output sizes are bounded before allocating, and the decoder is
  fuzzed against `miniz_oxide`. Faster decoding (streaming fused with
  base64, wide Huffman tables, SIMD copies) is kept only where it is
  safe. Otherwise we keep matching miniz rather than taking risks.
- **`fontdue` → our own rasterizer:** done for drawing (`src/fonts`: cmap,
  glyf, CFF/CFF2, a SIMD coverage rasterizer), with fontdue kept as the
  test oracle (`font_tests.rs`). Loading is lazy, so startup no longer
  parses every glyph of every fallback font (about 400 ms on a desktop
  with Nerd Fonts). Still to do: per-glyph rasterization, which is slower
  than fontdue's precomputed outlines (`font_engine_speed`); kerning;
  feeding the GPU glyph atlas. Color emoji (COLR/CBDT/sbix) is a separate
  step.
- **`softbuffer` → our own present path** (X11 MIT-SHM, Wayland `wl_shm`,
  macOS CALayer) for the software renderer, which becomes the fallback
  beside Vulkan/Metal. Do it together with the GPU work so presentation and
  damage handling are designed once.

Kept for now: `winit` (windowing, IME and DPI are too much risk for the
gain), `image` (wallpapers and iTerm2 inline images need JPEG and WebP), and `rfd`.

## Protocol gaps

- **GIF inline images.** iTerm2 inline images accept PNG, JPEG and WebP;
  GIF (often animated) needs our own decoder, since enabling the `image`
  crate's GIF support would add a dependency. Animated GIFs can reuse the
  kitty animation frames.
- **OSC 176 (application id)** is ignored: a Wayland app id is fixed once
  the window exists, so it cannot follow a program's request.
