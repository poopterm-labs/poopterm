PREFIX ?= /usr/local
DESTDIR ?=

BINDIR   = $(DESTDIR)$(PREFIX)/bin
ICONDIR  = $(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps
APPDIR   = $(DESTDIR)$(PREFIX)/share/applications
ICONROOT = $(DESTDIR)$(PREFIX)/share/icons/hicolor

all:
	cargo build --release

install: all
	install -d $(BINDIR) $(ICONDIR) $(APPDIR)
	install -m 755 target/release/poopterm $(BINDIR)/poopterm
	install -m 644 assets/icon.png $(ICONDIR)/poopterm.png
	install -m 644 packaging/poopterm.desktop $(APPDIR)/poopterm.desktop
	-gtk-update-icon-cache -q -t $(ICONROOT) 2>/dev/null

uninstall:
	rm -f $(BINDIR)/poopterm
	rm -f $(ICONDIR)/poopterm.png
	rm -f $(APPDIR)/poopterm.desktop

clean:
	cargo clean

.PHONY: all install uninstall clean
