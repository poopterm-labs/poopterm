#!/bin/sh
# Installs poopterm following the XDG layout so GNOME, KDE, and other
# desktops list it: binary, desktop entry, hicolor icons, and (system-wide
# only) the terminfo entry.
#
#   scripts/install.sh               # per user: ~/.local/bin, $XDG_DATA_HOME
#   PREFIX=/usr/local sudo -E scripts/install.sh
#   scripts/install.sh --uninstall   # same variables, removes the files
#
# PREFIX defaults to ~/.local. DESTDIR stages a system install for packaging.
# --no-build skips `cargo build`; POOPTERM_BIN picks the binary to install.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
action=install
build=1
for argument in "$@"; do
	case $argument in
	--uninstall) action=uninstall ;;
	--no-build) build=0 ;;
	-h | --help)
		sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "install.sh: unknown option: $argument" >&2
		exit 2
		;;
	esac
done

: "${HOME:?HOME is not set}"
PREFIX=${PREFIX:-$HOME/.local}
DESTDIR=${DESTDIR:-}
bindir=$PREFIX/bin
datadir=$PREFIX/share
# A per-user install puts data where the XDG Base Directory spec says.
case $PREFIX in
"$HOME" | "$HOME"/*)
	user=1
	case ${XDG_DATA_HOME:-} in
	/*) datadir=$XDG_DATA_HOME ;;
	esac
	;;
*) user=0 ;;
esac

icons="16x16:icon_16x16 32x32:icon_32x32 64x64:icon_32x32@2x 128x128:icon_128x128
256x256:icon_256x256 512x512:icon_512x512"

# Lets running desktops notice the change; harmless when the tools are missing.
refresh() {
	[ -z "$DESTDIR" ] || return 0
	command -v update-desktop-database >/dev/null 2>&1 &&
		update-desktop-database -q "$datadir/applications" 2>/dev/null
	command -v gtk-update-icon-cache >/dev/null 2>&1 &&
		gtk-update-icon-cache -q -t -f "$datadir/icons/hicolor" 2>/dev/null
	for tool in kbuildsycoca6 kbuildsycoca5; do
		command -v "$tool" >/dev/null 2>&1 && "$tool" >/dev/null 2>&1 && break
	done
	return 0
}

if [ "$action" = uninstall ]; then
	rm -f "$DESTDIR$bindir/poopterm" "$DESTDIR$datadir/applications/poopterm.desktop"
	for icon in $icons; do
		rm -f "$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps/poopterm.png"
	done
	if [ "$user" = 0 ]; then
		rm -f "$DESTDIR$datadir/terminfo/x/xterm-poopterm" "$DESTDIR$datadir/terminfo/p/poopterm" \
			"$DESTDIR$datadir/terminfo/78/xterm-poopterm" "$DESTDIR$datadir/terminfo/70/poopterm"
	fi
	refresh
	echo "Removed poopterm from $DESTDIR$PREFIX"
	exit 0
fi

binary=${POOPTERM_BIN:-$root/target/release/poopterm}
if [ "$build" = 1 ]; then
	(cd "$root" && cargo build --release --locked)
fi
[ -f "$binary" ] || {
	echo "install.sh: $binary not found; build it with cargo build --release" >&2
	exit 1
}

install -d "$DESTDIR$bindir" "$DESTDIR$datadir/applications"
install -m 755 "$binary" "$DESTDIR$bindir/poopterm"
# An absolute Exec works even when the session's PATH lacks ~/.local/bin.
sed -e "s|^Exec=poopterm|Exec=$bindir/poopterm|" -e "s|^TryExec=poopterm|TryExec=$bindir/poopterm|" \
	"$root/packaging/poopterm.desktop" >"$DESTDIR$datadir/applications/poopterm.desktop"
chmod 644 "$DESTDIR$datadir/applications/poopterm.desktop"
for icon in $icons; do
	install -d "$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps"
	install -m 644 "$root/assets/icon.iconset/${icon#*:}.png" \
		"$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps/poopterm.png"
done
# Only a system-wide entry is visible under sudo; see src/terminfo.rs.
if [ "$user" = 0 ] && command -v tic >/dev/null 2>&1; then
	install -d "$DESTDIR$datadir/terminfo"
	tic -x -o "$DESTDIR$datadir/terminfo" "$root/terminfo/poopterm.terminfo"
fi
refresh

echo "Installed poopterm to $DESTDIR$PREFIX"
case ":$PATH:" in
*":$bindir:"*) ;;
*) echo "Note: $bindir is not on your PATH; the desktop entry still works." ;;
esac
if [ "$user" = 1 ]; then
	echo "For TERM=xterm-poopterm (otherwise xterm-256color), install the terminfo"
	echo "entry system-wide: poopterm --print-terminfo | sudo tic -x -"
fi
