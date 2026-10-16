# poopterm 💩

Poopterm is a lightweight terminal emulator written in Rust. It opens your
default shell in a native window, with tabs, clipboard support, and seven built-in
color themes. Terminal compatibility is growing and is not yet a complete xterm
implementation.

![Poopterm app icon](assets/icon.png)

## Features

- Starts the shell in `$SHELL`, falling back to `/bin/sh`.
- Supports basic cursor movement, 16-color, 256-color, and truecolor SGR,
  alternate-screen switching, and common editing and function keys.
- Configurable scrollback (5,000 lines by default). Use the mouse wheel to browse it.
- Supports drag-to-select text and system clipboard copy/paste.
- Opens independent shell sessions in tabs, with activity and bell indicators.
- Searches scrollback with a highlighted match and next/previous navigation.
- Restores up to 12 tabs and their directories when reopening the window.
- Opens HTTP(S) links and copies displayed file paths with a modifier-click.
- Uses a compact tab bar and a status bar showing directory, grid size, and theme.
- Supports bracketed paste and common SGR mouse reporting modes.
- Resizes the shell along with the window.
- Batches synchronized terminal updates (`?2026`) to avoid partial frames.
- Displays bounded Sixel graphics and tracks OSC 133 shell markers.
- Supports semicolon/colon TrueColor, DEC line drawing, and additional text styles.
- Includes macOS Light and Dark, all four Catppuccin flavors, and signature Poop themes.
- Includes a desktop launcher and icon for Linux and FreeBSD.

## Build and run

Install stable Rust with [rustup](https://rustup.rs), then build from the
project directory:

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

Poopterm includes macOS Light and Dark, Catppuccin Latte, Frappé, Macchiato,
Mocha, and signature Poop themes. Rosé Pine has been removed for now. Choose a
theme in **Settings** or press **Ctrl+Shift+T** to cycle through them. The
selection is saved to a config file and restored on the next launch.

The default config path is `~/Library/Application Support/poopterm/config.toml`
on macOS and `~/.config/poopterm/config.toml` on Linux and FreeBSD (or
`$XDG_CONFIG_HOME/poopterm/config.toml` when `XDG_CONFIG_HOME` is set). Create
the file or open **Settings** from the gear button in the tab bar. Settings
opens in a separate window with a theme picker, font-size and scrollback controls,
and a session-restore toggle. Click **Save** to apply the preferences:

```toml
theme = "catppuccin-mocha"
font_size = 18           # Logical pixels, 10–32
scrollback_lines = 5000 # 0–100000; 0 disables history
restore_sessions = true
```

`theme` accepts `macos-dark`, `macos-light`, `catppuccin-latte`,
`catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, or `poop`.
Settings changes apply immediately after saving; session restore takes effect on
the next launch. Reducing scrollback discards the oldest lines. Invalid numbers
fall back to defaults or are clamped to supported limits. Other config keys and
comments are preserved when Settings saves. Toolbar text keeps its own size.

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
- **Cmd-click** / **Ctrl-click** an HTTP(S) URL to open it in your browser, or
  a displayed path containing `/` to copy it. Mouse-reporting terminal programs
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

On Linux, clipboard shortcuts need a clipboard command-line tool: `wl-clipboard`
for Wayland (`wl-copy` and `wl-paste`), or `xclip`/`xsel` for X11.

### Working directories and session restore

New tabs inherit the active shell's directory. macOS and Linux discover the
shell directory when opening a tab or closing the window. Shells can also report
it using OSC 7; BSD requires this for directory changes to be tracked.
Close the window to save its tabs; reopening starts fresh login shells in the
saved directories. Running programs and terminal output are not restored.
Closing a tab removes it from the saved list, including the last tab.
State is stored in `sessions.txt` next to the config file, with a limit of 12
restored tabs. Missing directories fall back to the home directory.

A dot marks unseen background output; `!` marks a terminal bell. Switching to a
tab clears its indicator.

## Install on Linux or FreeBSD

Install system-wide (requires administrator access):

```sh
sudo make install
```

Install for your user on Linux:

```sh
PREFIX="$HOME/.local" make install
```

For a user install, make sure `~/.local/bin` is in your `PATH`. To uninstall,
run `sudo make uninstall` for a system install, or use the same `PREFIX` as the
user install:

```sh
PREFIX="$HOME/.local" make uninstall
```

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
and truecolor output, wide-character cell spacing, text selection, clipboard
shortcuts, bracketed paste, and common mouse reporting. It remains a small emulator rather than a
complete xterm implementation; advanced terminal applications may rely on
sequences or input modes that are not supported yet. Combining marks are not
rendered. Nerd Font icon glyphs are not bundled, so applications that rely on
them may show missing characters.

The shell is currently told that the terminal is `xterm-256color`; apps may
therefore try features Poopterm does not support yet.

The 0.4.0 private-mode and line-drawing fixes improve full-screen programs,
including `htop`. Advanced features in `htop`, `btop`, and other TUIs may still
expose compatibility gaps. Please include the program version and a minimal
reproduction when reporting display problems.

## Font and license

Poopterm bundles DejaVu Sans Mono for Powerline as `assets/font.ttf`. The font is
distributed under the Bitstream Vera license, with public-domain changes. See
the [DejaVu font license](https://dejavu-fonts.github.io/License.html). Nerd
Font symbols are not loaded from system fonts; automatic fallback is not implemented.

Poopterm is MIT licensed; see [LICENSE](LICENSE).

## Modern terminal protocols

- **Synchronized output:** `ESC [ ? 2026 h` opens an update block; `ESC [ ? 2026 l`
  commits it. Grid changes stay hidden until the commit. A 250 ms timeout prevents
  a broken application from leaving the display frozen. `CSI ? 2026 $ p` queries
  the mode. See the [protocol specification](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md).
- **Shell integration:** OSC 133 `A`, `B`, `C`, and `D;exit_code` mark the prompt,
  command input, output, and completion. Poopterm retains the latest 256 markers
  and shows running/exit status in the status bar. Your shell or wrapper must
  emit these markers; Poopterm does not change startup files or inject scripts.
  See [OSC 133 semantics](https://iterm2.com/documentation-escape-codes.html).
- **Sixel:** DCS `q` images support RGB/HLS registers, repeated columns,
  transparency, raster dimensions, and integer pixel aspect ratios. Images are
  capped at 2,048 × 2,048 pixels and 1 MiB of encoded data. Each screen retains
  at most 16 images within a 4-megapixel budget. Images are discarded on text
  overwrite or scrolling out of view; image scrollback and partial bitmap
  erasure are not implemented. See [DEC's Sixel reference](https://vt100.net/docs/vt3xx-gp/chapter14.html).

## Development and releases

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
./target/release/poopterm --version
```

The code is separated into `state.rs` (grid/history), `parser.rs` (vte callbacks),
`osc.rs` (shell integration), `sixel.rs` (bounded image decoding), `config.rs`
(preferences), and `main.rs` (windowing, PTYs, input, rendering).
CI builds and checks macOS and Linux; BSD still needs testing on real systems.

Poopterm follows [Semantic Versioning](https://semver.org/): patch releases fix
bugs, minor releases add features, and major releases signal incompatible changes
after 1.0. Before 1.0, incompatible changes advance the minor version and are
listed in [CHANGELOG.md](CHANGELOG.md). Existing configuration keys, command-line
flags, and documented shell integration behavior are the compatibility surface.

Update `Cargo.toml`, the Poopterm entry in `Cargo.lock`, and the changelog together.
Successful CI on `main` creates the matching `vX.Y.Z` tag and GitHub release only
when the tag does not already exist. Published tags are never moved. Update the
Homebrew formula after that tag exists, so installs never point at missing source.
