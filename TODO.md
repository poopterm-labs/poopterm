# TODO

Open work, roughly in priority order. Measure before and after every step
(`docs/performance.md`); drop changes without a reproducible gain.

## Throughput: 200 MB/s end to end

This target depends on hardware and workload. The parser is now faster, but
not every payload has demonstrated 200 MB/s consistently on every machine.
Results and the Apple M4 test setup are in [`docs/performance.md`](docs/performance.md).

- **Rendering during floods:** frames still write unchanged background pixels,
  competing with parsing for memory bandwidth. Evaluate skipping those pixels
  or lowering internal resolution, retaining a change only with repeatable
  throughput and visual gains.
- **Reduce copies in PTY input:** an owned-buffer queue prototype did not show
  a repeatable gain across payloads and was removed. Revisit only with a
  benchmark that isolates copy cost while preserving queue backpressure.

## Rendering

- Selection and search highlights damage only rows whose highlight coverage
  changed. Pixel comparison against full redraw covers movement, clearing,
  scrolling, and Unicode text.
- During floods, test whether avoiding unchanged background writes improves
  throughput without harming visual correctness.

## Other

- Bash shell integration loads through a generated `--rcfile` wrapper.

## CI and platforms

- **Automatic monthly releases:** after every fully green CI run on `main`, CI
  increments the patch or starts a new month at `YY.M.0` with a new codename.
  It atomically pushes only if `main` still matches the checked revision,
  publishes the release, then updates the Homebrew formula. Release publishing
  and race handling are covered by local integration tests.
- **FreeBSD CI:** build and test in a FreeBSD VM with X11, libxkbcommon,
  Wayland, bash and zsh dependencies.
