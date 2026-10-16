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
- Keeps up to 5,000 lines of scrollback. Use the mouse wheel to browse it.
- Supports drag-to-select text and system clipboard copy/paste.
- Opens multiple independent shell sessions in tabs.
- Supports bracketed paste and common SGR mouse reporting modes.
- Resizes the shell along with the window.
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
opens in a separate window with buttons to cycle through themes, toggle Discord
Rich Presence, enter its Application ID, and set a symbol font fallback. Click
**Save** to write those controls to the config. The matching config keys are:

```toml
theme = "catppuccin-mocha"
discord_enabled = false
discord_client_id = "YOUR_DISCORD_APPLICATION_ID"
symbol_font = "/path/to/SymbolsNerdFontMono-Regular.ttf"
```

`theme` accepts `macos-dark`, `macos-light`, `catppuccin-latte`,
`catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, or `poop`.
`discord_enabled` controls whether Rich Presence runs; the Application ID stays
saved when the integration is turned off.
Settings changes apply immediately after saving; Discord presence reconnects
with the saved client ID. Other config keys are preserved when Settings saves.

Set `POOPTERM_CONFIG` to use a config file at another path. `POOPTERM_THEME`
can override the saved setting for a single launch, for example:

```sh
POOPTERM_THEME=catppuccin-mocha cargo run --release
```

```sh
POOPTERM_THEME=catppuccin-latte poopterm
```

### Discord Rich Presence

On macOS and Linux, Poopterm can show the active terminal window title in Discord
Rich Presence. Create a Discord application and add its Application ID to the
Poopterm config:

```toml
discord_client_id = "YOUR_DISCORD_APPLICATION_ID"
```

You can instead set `POOPTERM_DISCORD_CLIENT_ID` in the app's environment. The
presence follows the active terminal title (including titles set by shell or
terminal applications). It uses Discord's local IPC socket, so Vesktop works
when it can access that socket; Vesktop installed as a sandboxed Flatpak may
need host IPC access configured.

## Tabs and keyboard shortcuts

- **Cmd+T** on macOS or **Ctrl+Shift+N** elsewhere: open a tab.
- **Ctrl+Tab** / **Ctrl+Shift+Tab**: move to the next or previous tab.
- **Cmd+W** on macOS: close the active tab. Closing the last tab exits.
- Click a tab to switch sessions, or click **+** to open one.
- On macOS, tabs share the transparent titlebar beside the traffic lights. On
  Linux, they remain in the separate tab row.
- Click **Settings** in the tab bar to open the config editor. Click a tab's
  **×** to close it.
- **Cmd+C / Cmd+V** on macOS, or **Ctrl+Shift+C / Ctrl+Shift+V** on Linux:
  copy a selection or paste clipboard text.
- Drag with the left mouse button to select text. The mouse wheel scrolls
  scrollback unless the running app has enabled mouse reporting.
- **Ctrl+Shift+T** cycles themes and saves the choice to the config file.

On Linux, clipboard shortcuts need a clipboard command-line tool: `wl-clipboard`
for Wayland (`wl-copy` and `wl-paste`), or `xclip`/`xsel` for X11.

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

## macOS app bundle

`cargo run` launches the executable directly. App launches use a login shell,
so zsh loads both `~/.zprofile` and `~/.zshrc`. Put Homebrew's `shellenv` setup in
`~/.zprofile` so GUI-launched shells inherit its PATH. For Apple Silicon:

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
rendered. Nerd Font symbols use an installed Symbols Nerd Font Mono fallback
when one is available; set `POOPTERM_SYMBOL_FONT` to use a different fallback
font file.

The shell is currently told that the terminal is `xterm-256color`; apps may
therefore try features Poopterm does not support yet.

For system monitoring, we recommend [btop](https://github.com/aristocratos/btop)
instead of `htop`. `htop` can still have display issues in Poopterm, and advanced
terminal features in either program may expose remaining compatibility gaps.

## Font and license

Poopterm bundles DejaVu Sans Mono for Powerline as `assets/font.ttf`. The font is
distributed under the Bitstream Vera license, with public-domain changes. See
the [DejaVu font license](https://dejavu-fonts.github.io/License.html). Nerd
Font symbols are loaded from the system when available and are not bundled.

Poopterm is MIT licensed; see [LICENSE](LICENSE).
