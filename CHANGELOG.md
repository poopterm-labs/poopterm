# Changelog

## 0.42.0 — 2026-10-17

### Added

- Added a Custom theme with foreground, background, border, and ANSI color overrides; user-selected TrueType fonts with bundled glyph fallback.
- Added a searchable command palette, tab renaming, configurable keyboard shortcuts, and the About poopterm page in Settings.
- Added OSC 8 hyperlinks, local `file://` links, basic Unicode cluster retention, and initial Kitty keyboard protocol support.
- Added saved main-window opacity controls on macOS.
- Bundled Catppuccin's MIT license notice in `THIRD_PARTY_NOTICES`.

### Fixed

- Replaced the incomplete hand-rolled ANSI byte parser with vte's incremental parser and expanded SGR, OSC, and terminal mode handling.
- Improved terminal resize, alternate-screen, cursor, and color handling.
- Corrected Light/Dark theme labels and checked Catppuccin palette values against upstream.

## 0.4.0 — 2026-10-16

### Added

- Settings controls for font size (10–32), scrollback (0–100,000 lines), and session restore.
- Synchronized output (`CSI ? 2026 h/l`) with atomic state commits, mode queries, and a 250 ms recovery timeout.
- OSC 133 prompt, command, output, and exit-status tracking; the status bar reports running commands and exit codes when the shell emits these markers.
- Bounded Sixel decoding and rendering: RGB/HLS registers, repeats, transparency, raster attributes, aspect ratios, and cursor advancement.
- Colon-separated TrueColor, italic simulation, dim, underline, conceal, strikethrough, programmable tabs, and DEC line drawing.
- `--version` and `--help` without opening a GUI.
- macOS/Linux CI and immutable version tags/releases after successful checks on main.

### Fixed

- DEC private modes were incorrectly reading vte's ignore flag, breaking alternate screens, mouse tracking, bracketed paste, and cursor visibility.
- Deferred wrapping at the right margin, erase backgrounds, scroll regions, saved cursor attributes, and wide-character rendering/overwriting.
- Settings preserve unknown keys and comments, save atomically, and no longer duplicate headers.
- Terminal font changes no longer change toolbar typography.
- Search retains its selected match during output and accepts clipboard paste.
- Percent-encoded working directories and missing restored directories.

### Performance and maintenance

- Split terminal state, escape protocols, shell integration, Sixel, and configuration into modules.
- Bound pending PTY output and glyph caching; share scrollback and bitmap data across synchronized snapshots.
- Refresh search once per drawn frame; avoid redundant PTY flushes and reap closed child processes.
- Preserve user app icons and keep AUR packaging deferred.

### Limits

This is a growing emulator, not complete xterm compliance. Combining marks and basic ZWJ sequences stay with their base cell, but full grapheme shaping and complex-script shaping are not implemented. Kitty keyboard support covers the basic disambiguation mode rather than every progressive enhancement. Terminal image protocols other than Sixel remain unsupported. Sixel images are limited to 2,048 × 2,048 pixels, 1 MiB encoded data, 256 color registers, and 16 retained images within a 4-megapixel budget per screen. Images are discarded when their cells are overwritten or they scroll out of the visible region; image scrollback and partial bitmap erasure are not supported. Sixel aspect ratios are integer-rounded (1–10). The standard sixel display/scrolling mode 80 is supported. Shell integration requires shell-emitted OSC 133 markers; shell startup files are never modified automatically.
