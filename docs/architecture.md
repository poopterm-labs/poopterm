# Architecture

Poopterm is one process with a UI thread plus three threads per tab (PTY
reader, parser, PTY writer). Everything is drawn on the CPU into a softbuffer
surface.

## Data flow

```
            ┌──────────────────────────── per tab ─────────────────────────────┐
 shell ─pty─┤ reader ──► OutputQueue (1 MiB) ──► parser thread ──► SharedTerm  │
            │                                     │ replies ──► PtyWriter ─────┼─► shell
            └─────────────────────────────────────┼──────────────────▲─────────┘
                                         wake (Output)                │ lock: snapshot / state
 winit events ──────────────────────────►  App (UI thread) ───────────┘
                                           ├─ input encoders ─► PtyWriter ─► shell
                                           └─ FramePacer ─► draw: Frame ─► damage ─► canvas ─► softbuffer
```

1. **Reading.** `pty::spawn_session` starts a reader thread that appends each
   read to the tab's `OutputQueue`. If parsing falls 1 MiB behind, the reader
   waits, so a flood pushes back on the producing program instead of using
   memory. At end of file it closes the queue.
2. **Parsing.** The tab's parser thread (`pty::pump`) takes everything
   queued and applies it to the `term::SharedTerm` (a `Mutex<Term>`) in
   64 KiB chunks, yielding to a waiting UI thread between chunks. Replies the
   terminal owes the program (cursor reports, device attributes) go straight
   to `PtyWriter`. After each batch it sends `UserEvent::Output(id)` unless
   one is still unacknowledged, and it records whether the batch was a
   backlog (half the queue or more): a *flood*. When the queue is closed and
   drained it sends `UserEvent::Closed(id)`, so a tab's last output is never
   lost.
3. **Reacting.** `App::process_output` (in `app/output.rs`) acknowledges the
   wake-up, then briefly locks the terminal for OSC 52 clipboard writes,
   bell/title/cwd changes (compared with `Session::seen`) and long-command
   notifications.
4. **Drawing.** Output-driven redraws go through `app::FramePacer`: at most
   one frame per monitor refresh, or 30 fps while the active tab floods.
   Input-driven redraws are requested immediately. Each frame copies the
   visible rows into a `term::Frame` under the lock and renders from that
   copy (see Rendering).

The UI thread must not lock a terminal twice: `std::sync::Mutex` would
deadlock, so debug builds track held locks and panic instead.

## Terminal state (`src/term`)

- `Term` holds the screen `Grid`, scrollback (`VecDeque<Arc<Row>>`), cursor,
  modes, palette, tab stops, image placements and stored kitty images, and
  shell-integration markers.
- `Grid` stores one `Vec<Cell>` per row. Scrolling rotates row headers and
  moves the row leaving the screen into the scrollback, so a line feed costs
  O(columns), not O(screen).
- `Cell` is 16 bytes: character, foreground, background, attribute bits
  (`term::attr`), width (1 normal, 2 wide, 0 wide-character spacer) and an
  `extra` id. Combining marks and OSC 8 hyperlinks are interned in
  `term::extras::Extras`, shared copy-on-write with snapshots.
- **Synchronized output** (`?2026`): `begin_sync` clones the `Term` into a
  pending snapshot; all updates go to `Term::pending()` until the end marker
  or a 250 ms timeout, so the renderer never shows a half-drawn frame.
  Scrollback lines are `Arc`s, so the clone is cheap.
- The alternate screen swaps in a fresh `Grid` and keeps the primary one in
  `primary_screen`.

## Tokenizing (`src/vt`)

`vt::Parser` is poopterm's own escape-sequence tokenizer: the DEC/ECMA-48
state machine, written from the specifications, with SIMD scans
(`vt/simd.rs`, `u8x32`) that find where bulk work ends. It calls a
`vt::Handler`:

- **Text in runs.** Ground state scans to the next C0 control in one pass.
  Pure printable ASCII goes to `print_ascii(&[u8])`; anything else is
  validated once (`str::from_utf8`) and goes to `print(&str)`, with C1
  controls (U+0080–U+009F) split out and executed.
- **Payloads in slices.** OSC fields, DCS payloads (`put(&[u8])`, Sixel) and
  APC strings (`apc_start` / `apc_put` / `apc_end`, the kitty graphics
  protocol) are scanned in bulk, not byte by byte. OSC strings over 4 MiB
  are dropped.
- **CSI parameters in a tight loop** (`sequence.rs`), entered straight from
  ground state on `ESC [`. Anything unusual falls back to the general
  per-byte state machine, so both paths always agree.
- **Split reads.** A UTF-8 character split across reads is completed from
  the next one; sequences carry their state across reads.

Semantics match vte 0.15, which stays as a dev-dependency: `vt/tests.rs`
feeds random streams through both and requires identical events, with our
side split into random reads (and byte-by-byte). Known differences on
purpose: APC strings are delivered (vte drops them), and characters split
across reads are not corrupted (vte 0.15 drops bytes after them).

## Applying sequences (`src/parser`)

`parser::Parser` wraps `vt::Parser` with `Term` as the handler. The
per-family handlers (`csi.rs`, `esc.rs`, `osc.rs`, `sgr.rs`, `print.rs`)
are `impl Term` blocks. `batched_printing_matches_per_character_printing`
feeds random streams through `parser::Parser` and through vte driving
`Term` one character at a time (`parser::PerCharacter`), and requires
identical terminal state.

## Images (`src/graphics`, `src/sixel.rs`, `src/term/kitty*.rs`)

Both image protocols end in `graphics::Placement`s on `Term::images` (the
alternate screen keeps the primary screen's in `primary_images`): an
`Arc<Bitmap>` of straight-alpha `0xAARRGGBB` pixels, a grid position (the
row goes negative once scrolled into history), a source rectangle, an
extent, a z-index and, for kitty images, the image and placement ids.
`Term::scroll_images` moves placements with the text. Sixel images are
erased by text written over them; kitty images only by delete commands,
`ED 2`/`ED 3`, RIS and leaving the alternate screen.

The kitty graphics protocol runs on the parser thread:

- `vt` hands APC strings over in slices; `kitty::Receiver` checks the `G`,
  parses the control keys (`kitty::Command`), decodes base64 as it arrives
  (`graphics::base64`, SIMD for whole blocks) and joins chunked transfers.
  Limits are enforced while receiving, before anything is decoded.
- `kitty::load` reads file, temporary-file and shared-memory media;
  `kitty::decode` turns RGB, RGBA and PNG (optionally zlib) into a `Bitmap`.
- `kitty::Store` keeps images by id and number with a quota and least
  recently used eviction that prefers images not on screen. Each image is an
  `Animation`: its frames, the shown frame and the playback timeline.
  Virtual placements (`U=1`) live there too.
- `term/kitty.rs` applies transmit, place and delete; `term/kitty_animation.rs`
  applies frames, composition and playback. Replies go to `Term::responses`.
- Running animations are advanced by the UI thread (`about_to_wait`), which
  sleeps until the next frame of an animation that is on screen.
- Unicode placeholder cells (U+10EEEE) store the image id as the client set
  the foreground color (`Term::fg_id`). `Term::snapshot` resolves them into
  `Frame::placeholders`, and only while virtual placements exist.

The renderer draws each row in layers: images below `i32::MIN / 2`, cell
backgrounds, other negative z-indices, text and placeholder slices, then
z-index 0 and above. Images are clipped per row, so damage tracking can
redraw single rows; `render::damage` compares placements by bitmap identity
and position, and placeholder slices row by row.

## Rendering (`src/render`, `src/app/draw*.rs`)

- `render::painter::Painter` wraps the frame buffer with clipped `rect`,
  `round_rect`, `button`, `text` and `glyph` operations.
- `render::glyphs::FontSet` owns the primary and fallback fonts and caches
  rasterized glyphs by `(char, size)` in an `FxHashMap`; the fallback font is
  resolved only on a cache miss.
- `render::grid::draw_grid` draws a `GridView`: cells right to left (so wide
  glyphs paint over their spacer), cursor, selection, search match, underline
  and strike styles, hyperlinks, combining marks and images.
- `render::blend` blends glyph coverage four pixels at a time with
  `portable_simd`.
- `app/draw.rs` composes a full frame: background or wallpaper, tab bar
  (`draw_tabs.rs`), terminal grid, search box, command palette, status bar.
  The Settings window is drawn by `app/settings_view.rs`.

**Damage tracking.** `app::canvas::Canvas` keeps a back buffer holding the
last frame and the `Frame` it shows. `render::damage::grid_damage` compares
the new `Frame` with it row by row (cells, marks, links, cursor).
When lines scrolled into history (`Frame::top_line` grew), the old pixels are
shifted up and only the new rows are drawn; with a wallpaper, rows are
redrawn instead, because the wallpaper does not scroll. `app/repaint.rs`
restores the background under damaged rows, redraws them and the tab and
status bars, and copies only changed pixel rows to the window buffer. A full
frame is drawn after resizes, theme or font changes, on tab switches, focus
changes, and while a selection, search match or overlay is shown. Glyphs are
clipped to their row so rows can be redrawn independently. X11 windows get
the whole image each present (they lose contents when exposed); Wayland and
macOS get damage rectangles.

## Input (`src/input`)

All encoders are pure functions with unit tests:

- `keys::encode_key(key, modifiers, modes)`: legacy xterm and kitty
  (`CSI u`) encodings, Alt as ESC prefix, modified cursor and function keys,
  DECCKM application mode.
- `mouse::encode_mouse`: SGR (`?1006`) and X10 reports with modifier bits.
- `paste::paste_bytes`: CR line endings; strips paste markers from pasted
  text when bracketed paste is on.
- `scroll::ScrollAccumulator`: touchpad pixel deltas to whole lines;
  `alternate_scroll_bytes` for wheel scrolling in alternate-screen programs.
- `shortcuts::Shortcuts`: configurable chords, parsed once from the config.

## Configuration and persistence

- `config.rs`: a small TOML-like `key = value` format. Saving rewrites only
  the changed keys and keeps unknown keys and comments.
- `session_store.rs`: tab working directories in `sessions.txt` (hex-encoded
  paths) next to the config file.
- `theme.rs`: built-in palettes and the xterm 256-color cube.
