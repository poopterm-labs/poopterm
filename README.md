# poopterm 💩

A tiny, fully slopcoded terminal emulator, written in Rust. (this is the worst terminal of all time, i love it.)

![poopterm icon](assets/icon.png)

## Features

- PTY-backed shell (`$SHELL`, falls back to `/bin/sh`)
- 16-color SGR support and basic cursor movement
- Caramel rim window with macOS traffic lights (transparent title bar)
- Resizes the shell PTY with the window
- Desktop launcher and icon for Linux and FreeBSD

## Requirements

- Rust (stable), installed via [rustup](https://rustup.rs)
- macOS, Linux, or FreeBSD with X11 (Wayland works on Linux)

### Linux (Debian/Ubuntu)

```bash
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev
```

Fedora: `sudo dnf install gcc pkgconf-pkg-config libX11-devel libxkbcommon-devel wayland-devel`

Arch: `sudo pacman -S base-devel libx11 libxkbcommon wayland`

### FreeBSD

```sh
pkg install rust pkgconf
```

Plus `xorg` or `libX11`, and `libxkbcommon` and `wayland` for Wayland.

## Build and run

From the project root:

```bash
cargo run --release
```

## Install

System-wide (Linux or FreeBSD, needs root):

```bash
make
sudo make install
```

On FreeBSD, use `su -m root -c 'make install'` or `doas make install`.

Per-user, no root (Linux):

```bash
PREFIX=$HOME/.local make install
```

Make sure `~/.local/bin` is in your `PATH`. To remove it, run `make uninstall` with the same `PREFIX`.

## macOS app bundle

`cargo run` launches a bare binary, so macOS shows a generic Dock icon. To get the real icon, build a `.app` bundle from the release binary, the `.icns` file in `assets/`, and an `Info.plist`, then open `poopterm.app`.

## Font

Uses DejaVu Sans Mono for Powerline, bundled as `assets/font.ttf`. DejaVu fonts are distributed under the Bitstream Vera license with public domain changes. See https://dejavu-fonts.github.io/License.html.

## Known limitations

- No scrollback yet
- No alternate screen, so `vim` and `htop` render roughly
- Some symbols show as boxes because the font lacks them
- Wide characters (emoji, some CJK) take one column, so layout can drift
- No text selection or copy/paste yet

## License

MIT, see [LICENSE](LICENSE). The bundled font has its own license.
