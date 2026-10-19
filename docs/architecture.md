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
  modes, palette, tab stops, Sixel placements and shell-integration markers.
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

## Parsing (`src/parser`)

`vte` turns bytes into callbacks (`print`, `execute`, `csi_dispatch`, …).
Poopterm adds three layers around it, all in `parser/mod.rs`:

1. **Split UTF-8 carry.** A trailing incomplete UTF-8 sequence is held back
   and completed from the next read. vte 0.15 drops up to two bytes after a
   character split across `advance` calls, so it must only see whole
   characters.
2. **Ground-state fast path.** While vte is known to be in its ground state,
   `Parser::feed` scans with `u8x32` SIMD compares (`parser/scan.rs`) and
   handles printable ASCII runs (`Term::print_ascii`) and C0 controls itself.
   On ESC or a non-ASCII byte it hands over to vte with
   `advance_until_terminated`, stopping at the first callback after which vte
   is always back in ground state: CSI or ESC dispatch, a BEL-terminated OSC,
   or CAN/SUB. Any other path keeps using vte, which is always correct.
3. **Batching.** On the vte path, `Batch` collects consecutive printable ASCII
   `print` calls into runs for `Term::print_ascii`.

The per-family handlers (`csi.rs`, `esc.rs`, `osc.rs`, `sgr.rs`, `print.rs`)
are `impl Term` blocks. A differential test feeds random streams through the
fast path and through vte driving `Term` directly, and requires identical
results.

## Rendering (`src/render`, `src/app/draw*.rs`)

- `render::painter::Painter` wraps the frame buffer with clipped `rect`,
  `round_rect`, `button`, `text` and `glyph` operations.
- `render::glyphs::FontSet` owns the primary and fallback fonts and caches
  rasterized glyphs by `(char, size)` in an `FxHashMap`; the fallback font is
  resolved only on a cache miss.
- `render::grid::draw_grid` draws a `GridView`: cells right to left (so wide
  glyphs paint over their spacer), cursor, selection, search match, underline
  and strike styles, hyperlinks, combining marks and Sixel images.
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
