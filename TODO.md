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
window. Latest i3-N305 numbers are ~55–110 MB/s depending on the payload;
see [`docs/performance.md`](docs/performance.md).

**Images: as fast as possible** for Sixel and the kitty graphics protocol,
from first byte to pixels on screen. There is no target to stop at; at least
match text throughput, within the explicit size limits on untrusted input.

- **Own escape-sequence parser.** Replace `vte` with a hand-rolled,
  SIMD-classified, batch-oriented parser; vte's per-character dispatch and
  UTF-8 validation are most of the parser time on Unicode output, and vte
  ignores APC strings, which the kitty graphics protocol needs. Design and
  phased plan: [`docs/vte-design.md`](docs/vte-design.md). Each phase is
  verified differentially against vte before it lands.
- **Image benchmark.** Add Sixel and kitty-graphics payloads (small icons,
  full-screen frames, animation) to `bench-throughput.py`, reporting MB/s of
  encoded input and frames per second, and to the parser benchmark.
- **Sixel decoding.** Profile `src/sixel.rs` against that benchmark; candidates
  are SIMD band decoding, decoding straight into the placement bitmap, and
  streaming decode instead of buffering the whole DCS.
- **Kitty graphics protocol.** Transmission (direct, chunked `m=1`; file
  and shared memory later), placement, deletion, and SIMD base64 and
  zlib decoding, with explicit size limits like Sixel's.
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

Full GPU rendering, after kitty graphics lands. CPU drawing is the main
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

## CI and platforms

- **First automatic release.** The release job (version bump, GitHub
  release, Homebrew formula update) has never run: the FreeBSD job hung on
  an interactive-bash test until it was fixed. Confirm the next green `main`
  run publishes a release and points `Formula/poopterm.rb` at it (it is still
  at 0.43.0).
