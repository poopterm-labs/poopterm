# poopterm 💩

Poopterm is a lightweight, high-throughput terminal emulator written in Rust: it parses plain text at about 100 MB/s end to end on a low-power laptop CPU (see [performance](docs/performance.md)). It opens your default shell in a native window, with tabs, clipboard support, and seven built-in color themes. Terminal compatibility is growing and is not yet a complete xterm implementation.

![Poopterm app icon](assets/icon.png)

## Features

- Starts the shell in `$SHELL`, falling back to `/bin/sh`.
- Supports basic cursor movement, 16-color, 256-color, and truecolor SGR,
  alternate-screen switching, and common editing and function keys.
- Configurable scrollback (5,000 lines by default). Use the mouse wheel to browse it.
- Supports drag-to-select text and system clipboard copy/paste.
- Keeps combining marks attached to their base cell for display and copy, and can load a user font with the bundled font as a missing-glyph fallback.
- Opens independent shell sessions in tabs, with activity and bell indicators.
- Searches scrollback with a highlighted match and next/previous navigation.
- Restores up to 12 tabs and their directories when reopening the window.
- Opens HTTP(S) links, OSC 8 hyperlinks, and local `file://` links with a modifier-click; modifier-clicking a displayed path copies it.
- Uses a compact tab bar and a status bar showing directory, grid size, and theme.
- Supports bracketed paste and common SGR mouse reporting modes.
- Supports the initial Kitty keyboard protocol (`CSI u`) key encoding for applications that opt in.
- Resizes the shell along with the window.
- Batches synchronized terminal updates (`?2026`) to avoid partial frames.
- Displays Sixel images and the kitty graphics protocol (including animation
  and Unicode placeholders) within explicit memory limits, and tracks OSC 133
  shell markers.
- Supports semicolon/colon TrueColor, DEC line drawing, and additional text styles.
- Includes Dark Mode, Light Mode, all four Catppuccin flavors, signature Poop, and a configurable Custom theme.
- Includes an **About poopterm** page in Settings.
- Supports PNG, JPEG, and WebP terminal wallpapers with adjustable strength in Settings.
- Lets macOS users adjust main-window opacity from Settings.
- Includes a desktop launcher and icon for Linux and FreeBSD.
- **Fast output:** a background PTY reader with backpressure, redraws paced to the display, an O(columns) row-based grid with 16-byte cells, and a portable-SIMD fast path for plain text. Run `scripts/bench-throughput.py` in any terminal to compare.

## Build and run

Poopterm currently requires **nightly Rust** because its optimized pixel blending uses Rust's `portable_simd` feature. The included `rust-toolchain.toml` selects nightly automatically when Rust is installed through rustup.

Build from the project directory:

```sh
cargo run --release
```

To build without launching the app, run `cargo build --release`.

## Package managers

The Homebrew formula is available from the Poopterm tap. Install it with:

```sh
brew tap poopterm-labs/poopterm https://github.com/poopterm-labs/poopterm.git
brew install poopterm-labs/poopterm/poopterm
```

On macOS, Homebrew builds both the `poopterm` command and a `Poopterm.app`
bundle containing the `.icns` app icon. The bundle lives in Homebrew's managed
prefix. Open it directly with:

```sh
open "$(brew --prefix)/opt/poopterm/Poopterm.app"
```

To make the app appear in your user Applications folder, create a symlink:

```sh
mkdir -p ~/Applications
ln -sfn "$(brew --prefix)/opt/poopterm/Poopterm.app" ~/Applications/Poopterm.app
```

The link follows Homebrew upgrades. The `poopterm` command also remains
available for launching the app from a terminal.

### Build dependencies

On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev
```

On Fedora:

```sh
sudo dnf install gcc pkgconf-pkg-config libX11-devel libxkbcommon-devel wayland-devel
```

On Arch Linux:

```sh
sudo pacman -S base-devel libx11 libxkbcommon wayland
```

On macOS, install the Xcode Command Line Tools if they are not already present:

```sh
xcode-select --install
```

On FreeBSD, install Rust, pkgconf, and the display-system libraries you use:

```sh
pkg install rust pkgconf xorg libX11 libxkbcommon wayland
```

## Themes

Poopterm includes Dark Mode and Light Mode, Catppuccin Latte, Frappé, Macchiato,
Mocha, and signature Poop themes. Rosé Pine has been removed for now. Choose a
theme in **Settings** or press **Ctrl+Shift+T** to cycle through them. The
selection is saved to a config file and restored on the next launch.

The default config path is `~/Library/Application Support/poopterm/config.toml`
on macOS and `~/.config/poopterm/config.toml` on Linux and FreeBSD (or
`$XDG_CONFIG_HOME/poopterm/config.toml` when `XDG_CONFIG_HOME` is set). Create
the file or open **Settings** from the gear button in the tab bar. Settings
opens in a separate window with a theme picker, wallpaper chooser and strength
control, font-size and scrollback controls, and a session-restore toggle. Click
**Save** to apply the preferences:

```toml
theme = "catppuccin-mocha"
font_size = 18           # Logical pixels, 10–32
scrollback_lines = 5000 # 0–100000; 0 disables history
restore_sessions = true
opacity = 100           # macOS main window opacity, 60–100
wallpaper = "/Users/you/Pictures/terminal.webp" # optional PNG, JPEG, or WebP
wallpaper_strength = 35 # 0–100; 0 hides the image, 100 shows it at full strength
shortcut_new_tab = "super+t"
shortcut_command_palette = "ctrl+shift+p"
shortcut_cycle_theme = "ctrl+shift+t"
shortcut_search = "super+f"
shortcut_close_tab = "super+w"
```

`theme` accepts `darkmode`, `light-mode`, `catppuccin-latte`,
`catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, `poop`, or
`custom`. The custom theme starts from the Poop palette and accepts optional
six-digit RGB overrides. Color keys can also override a built-in theme:

```toml
theme = "custom"
foreground = "#E8D5B0"
background = "#24170B"
border = "#7A4E1F"
color0 = "#1E1409"
color1 = "#D9534F"
color2 = "#8BC34A"
color3 = "#E0B040"
color4 = "#5C9BD6"
color5 = "#B06AC8"
color6 = "#4FB6B0"
color7 = "#E8D5B0"
font_path = "/path/to/YourNerdFontMono-Regular.ttf"
```

`font_path` or the `POOPTERM_FONT` environment variable selects a TrueType font
file; glyphs missing from a custom font fall back to the bundled DejaVu Sans Mono
for Powerline font. Poopterm does not search system fonts automatically. For
complete htop or other Nerd Font icon coverage, set `font_path` to a Nerd Font
Mono file. Use a monospaced font for predictable cell alignment.
`POOPTERM_FONT` takes precedence over `font_path`.
The older `macos-dark` and `macos-light` values remain accepted for existing configs.
Settings changes apply immediately after saving; session restore takes effect on
the next launch. Reducing scrollback discards the oldest lines. Invalid numbers
fall back to defaults or are clamped to supported limits. Other config keys and
comments are preserved when Settings saves. Toolbar text keeps its own size.

On macOS, use the **Window opacity** controls in Settings and save to make the
main window translucent. This applies to the entire terminal window; the separate
Settings window stays opaque. The default is 100%. This setting currently works
on macOS only; blur is not implemented. Catppuccin palette attribution and license terms are included in
`THIRD_PARTY_NOTICES` and in packaged app resources.

Set `POOPTERM_CONFIG` to use a config file at another path. `POOPTERM_THEME`
can override the saved setting for a single launch, for example:

```sh
POOPTERM_THEME=catppuccin-mocha cargo run --release
```

```sh
POOPTERM_THEME=catppuccin-latte poopterm
```

## Tabs and keyboard shortcuts

- **Cmd+T** on macOS or **Ctrl+Shift+N** elsewhere: open a tab.
- **Ctrl+Tab** / **Ctrl+Shift+Tab**: move to the next or previous tab.
- **Cmd+W** on macOS or **Ctrl+Shift+W** elsewhere: close the active tab.
- **Cmd+1–9** / **Ctrl+1–9**: jump directly to a tab.
- **Cmd+F** / **Ctrl+Shift+F**: search the current tab's scrollback. Type a query,
  press **Enter** / **Shift+Enter** for next/previous, and **Esc** to close.
  Search is case-insensitive for ASCII and matches within each displayed row
  (up to 10,000 matches per query).
  The usual paste shortcut can paste a search query.
- **Cmd+Up/Down** on macOS or **Ctrl+Shift+Up/Down** elsewhere: jump to the
  previous or next shell prompt (needs shell integration).
- **Cmd-click** / **Ctrl-click** an HTTP(S) URL to open it in your browser, or
  a displayed path containing `/` to copy it. Links to executables and
  launcher files (`.desktop`, `.command`, …) are copied, never opened; the
  status bar shows a hyperlink's real target while you hover it. Mouse-reporting terminal programs
  retain their own click handling.
- Click a tab to switch sessions, or click **+** to open one.
- On macOS, tabs share the transparent titlebar beside the traffic lights. On
  Linux, they remain in the separate tab row.
- Click the gear button in the tab bar to open Settings. Click a tab's
  **×** to close it.
- **Cmd+C / Cmd+V** on macOS, or **Ctrl+Shift+C / Ctrl+Shift+V** on Linux:
  copy a selection or paste clipboard text.
- Drag with the left mouse button to select text. The mouse wheel scrolls
  scrollback unless the running app has enabled mouse reporting.
- **Ctrl+Shift+T** cycles themes and saves the choice to the config file.
- **Ctrl+Shift+P** opens the command palette for new tab, settings, About, theme,
  copy-last-command-output, close-tab, and rename-tab actions. Enter filters and runs the first match; after
  choosing rename, type a title and press Enter.

The `shortcut_new_tab`, `shortcut_command_palette`, `shortcut_cycle_theme`,
`shortcut_search`, `shortcut_close_tab`, `shortcut_previous_prompt`, and
`shortcut_next_prompt` config keys accept `ctrl`, `shift`, `alt`, and `super`
plus a character key or one of `up`, `down`, `left`, `right`, `pageup`,
`pagedown`, `home`, `end`, `tab`, `enter`, `space`. Defaults use the platform's common
shortcuts; the sample values above are for macOS.

On Linux, clipboard shortcuts need a clipboard command-line tool: `wl-clipboard`
for Wayland (`wl-copy` and `wl-paste`), or `xclip`/`xsel` for X11.

### Shell integration

Poopterm's hooks report command start/exit and the current working directory.
They power prompt jumps, "Copy last command output", and a tab marker (plus a window attention request) when a
command of 10 s or more finishes while you are not looking.

**zsh, fish, and bash load them automatically**, without editing your startup files
(zsh through a temporary `ZDOTDIR` that then restores yours, fish through
`XDG_DATA_DIRS`, bash through `--rcfile`). Bash's loader reads `/etc/profile`
and the first available login profile before installing hooks, or `.bashrc`
when no login profile exists. Bash ignores `--rcfile` with `-l`, so this shell
is interactive but does not set bash's `login_shell` flag. Set `shell_integration = "off"` in `config.toml` to disable
this. The hooks also make `ssh` connect with `TERM=xterm-256color`, since
remote hosts rarely have poopterm's terminfo entry; set
`POOPTERM_SSH_KEEP_TERM=1` to keep it.

To install the hooks by hand, review the output and append it to
the startup file for the shell you use:

```sh
poopterm --print-shell-integration zsh >> ~/.zshrc
poopterm --print-shell-integration bash >> ~/.bashrc
poopterm --print-shell-integration fish >> ~/.config/fish/config.fish
```

Use only the command matching your shell. Restart the shell after adding it.
Remove the generated block between the `poopterm shell integration` comments to
disable the hooks. Without them, Poopterm still launches the
shell and tracks OSC 7/133 markers emitted by other integrations.

### Working directories and session restore

New tabs inherit the active shell's directory. macOS and Linux discover the
shell directory when opening a tab or closing the window. Shells can also report
it using OSC 7. On BSD, install the optional shell hooks above to report
directory changes.
Close the window to save its tabs; reopening starts fresh login shells in the
saved directories. Running programs and terminal output are not restored.
Closing a tab removes it from the saved list, including the last tab.
State is stored in `sessions.txt` next to the config file, with a limit of 12
restored tabs. Missing directories fall back to the home directory.

A dot marks unseen background output; `!` marks a terminal bell. Switching to a
tab clears its indicator.

## Install on Linux or FreeBSD

Install system-wide, so every user, desktop session, and `sudo` finds
Poopterm: the binary goes to `/usr/local/bin`, the desktop entry and icons to
`/usr/local/share`, so GNOME, KDE, and other desktops list it in their app
menus, and the terminfo entry to `/usr/local/share/terminfo`. Run the script
as yourself: it builds with cargo and asks for root (sudo or doas) only to
copy the files.

```sh
scripts/install.sh
scripts/install.sh --uninstall
```

`make && sudo make install` does the same. `PREFIX` and `DESTDIR` are
honored, for packaging. Poopterm keeps its config in `$XDG_CONFIG_HOME/poopterm`, open tabs in `$XDG_STATE_HOME/poopterm`,
and generated files in `$XDG_DATA_HOME/poopterm`.

## Login shell

Poopterm launches the shell from `$SHELL` (falling back to `/bin/sh`) as a login
shell on macOS, Linux, and BSD. For zsh, this loads both `~/.zprofile` and
`~/.zshrc`.

## macOS app bundle

Put Homebrew's `shellenv` setup in `~/.zprofile` so GUI-launched shells inherit
its PATH. For Apple Silicon:

```sh
eval "$(/opt/homebrew/bin/brew shellenv)"
```

For Intel Macs, use `/usr/local/bin/brew` instead. Commands such as `hyfetch`
must be installed and available on the PATH in that login shell.

## Compatibility notes

Poopterm supports alternate-screen programs such as `nano` and `btop`, common
cursor and editing sequences, device status replies, scroll regions, 256-color
and truecolor output, wide-character cell spacing, combining marks, text selection,
clipboard shortcuts, bracketed paste, and common mouse reporting. It remains a
small emulator rather than a complete xterm implementation; advanced terminal
applications may rely on sequences or input modes that are not supported yet.
Nerd Font icon glyphs are not bundled, but you can select a Nerd Font Mono file
with `font_path` or `POOPTERM_FONT`.

## Font and license

Poopterm bundles DejaVu Sans Mono for Powerline as `assets/font.ttf`. The font is
distributed under the Bitstream Vera license, with public-domain changes. See
the [DejaVu font license](https://dejavu-fonts.github.io/License.html). A custom
font falls back to the bundled font for missing glyphs; Poopterm also searches
common system font directories for up to eight TTF fonts with Nerd Font, Symbol,
or Awesome in their filenames. For the broadest htop icon coverage, set
`font_path` to a Nerd Font Mono file.

Poopterm is MIT licensed; see [LICENSE](LICENSE).

## Modern terminal protocols

- **Synchronized output:** `ESC [ ? 2026 h` opens an update block; `ESC [ ? 2026 l`
  commits it. Grid changes stay hidden until the commit. A 250 ms timeout prevents
  a broken application from leaving the display frozen. `CSI ? 2026 $ p` queries
  the mode. See the [protocol specification](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md).
- **Shell integration:** OSC 133 marks prompts, command input, output and
  completion, following the [semantic prompts proposal](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md):
  `L` and `A`/`N` start a fresh line, `P` and `k=` mark continuation and right
  prompts, `I` works like `B`, `aid=` tracks nested programs so `N` finishes
  the right command, and `D` takes an exit code and `err=`. With `cl=`, a plain
  click in the line being edited moves the cursor with arrow keys. kitty's
  `cmdline=` and `cmdline_url=` on `C` name the running command. Poopterm
  retains the latest 256 markers and shows the running command or exit status
  in the status bar. Optional hooks are available for zsh, Bash, and fish;
  install them explicitly with the instructions below. The hooks also emit
  OSC 7 so BSD shells can report their working directory.
- **OSC 52:** Clipboard writes to the `c` selection are supported up to 1 MiB.
  Clipboard read requests are ignored so shell programs cannot read clipboard
  contents without consent.
- **Sixel:** DCS `q` images support RGB/HLS registers, repeated columns,
  transparency, raster dimensions, and integer pixel aspect ratios. Defensive
  limits cap an image at 4,096 × 4,096 pixels, 16 megapixels, and 4 MiB encoded.
  Each screen retains at most 16 images and evicts older images to keep its
  retained bitmap budget near 4 megapixels; a single accepted image can exceed
  that budget. Oversized or malformed transfers are discarded. Images scroll
  into the scrollback with the text and are dropped when they leave it; text
  written over an image erases the whole image (partial bitmap erasure is not
  implemented). See [DEC's Sixel reference](https://vt100.net/docs/vt3xx-gp/chapter14.html).
- **Kitty graphics:** APC `G` commands transmit RGB, RGBA and PNG data,
  optionally zlib-compressed, directly (chunked with `m=1`), from a file, a
  temporary file or shared memory; place, query and delete images (every
  `d=` selector, with uppercase freeing); and support z-index layering under
  or over text and backgrounds, relative placements, Unicode placeholders
  (`U=1`), animation frames, composition and terminal-driven playback.
  Images scroll into the scrollback and are cleared by `ED 2`, RIS and the
  alternate screen as the protocol specifies. Limits: 256 MiB per
  transmission, 16,384 pixels per side and 64 megapixels per image, 320 MiB
  of decoded images and 4,096 images per terminal (least recently used,
  off-screen images are evicted first), 65,536 frames per image. File
  transmissions accept only absolute paths outside `/proc`, `/sys` and
  `/dev` (except `/dev/shm`). See the [protocol specification](https://sw.kovidgoyal.net/kitty/graphics-protocol/).

## Terminfo

Shells run with `TERM=xterm-poopterm`, an entry that advertises only what
poopterm supports (truecolor, styled underlines, synchronized output, focus
events, …), when it is installed system-wide; otherwise they get
`TERM=xterm-256color`. A per-user copy (`~/.terminfo`) is not used, because
`sudo` cannot see it and programs such as `sudo dmesg` would lose their
colors. `make install` installs the entry system-wide, or run
`poopterm --print-terminfo | sudo tic -x -`. Set `term = "xterm-256color"`
(or any name) in `config.toml` to override.

## Development and releases

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
./target/release/poopterm --version
```

Development is test-driven. Start with [`AGENTS.md`](AGENTS.md) (for humans
and AI coding agents alike), then see [architecture](docs/architecture.md),
[testing](docs/testing.md) and [performance](docs/performance.md).
CI builds and checks macOS, Linux, and a real FreeBSD VM.

Releases are numbered `YY.M.PATCH`. After all checks pass on `main`, CI
increments the patch, or starts a new month at `YY.M.0` with a new random
codename. It updates `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` together,
publishes the tag and release, then updates the Homebrew formula. It skips
stale CI runs and atomically rejects a push if `main` moved in the meantime.
Published tags never move. These commits use `GITHUB_TOKEN` and do not trigger
another workflow. Run `git pull --rebase` before pushing work to `main`.
