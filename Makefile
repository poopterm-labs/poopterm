# System-wide install; for a per-user install run scripts/install.sh directly.
PREFIX ?= /usr/local
DESTDIR ?=

all:
	cargo build --release --locked

install: all
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --no-build

uninstall:
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --uninstall

clean:
	cargo clean

.PHONY: all install uninstall clean
