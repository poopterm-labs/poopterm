# Builds and installs system-wide through scripts/install.sh; the pgo
# targets build a profile-guided optimized binary through scripts/pgo.sh.
#
#   make                 release build
#   make install         release build, installed to $(PREFIX)
#   make pgo             instrument, train on the benchmark, build optimized
#   make pgo-use         run the instrumented terminal; your session trains it
#   make pgo-build       rebuild optimized from every profile so far
#   make pgo-install     install the optimized build
#   make pgo-compare     benchmark the release and PGO builds
#   make pgo-clean       forget all profiles
PREFIX ?= /usr/local
DESTDIR ?=
# MiB per benchmark workload when training.
PGO_MIB ?= 8

all:
	cargo build --release --locked

install: all
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --no-build

uninstall:
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --uninstall

pgo:
	sh scripts/pgo.sh all $(PGO_MIB)

pgo-use:
	sh scripts/pgo.sh use

pgo-build:
	sh scripts/pgo.sh build

pgo-install:
	@test -x "$$(sh scripts/pgo.sh path)" || sh scripts/pgo.sh all $(PGO_MIB)
	POOPTERM_BIN="$$(sh scripts/pgo.sh path)" PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) \
		sh scripts/install.sh --no-build

pgo-compare:
	sh scripts/pgo.sh compare

pgo-clean:
	sh scripts/pgo.sh clean

clean:
	cargo clean

.PHONY: all install uninstall pgo pgo-use pgo-build pgo-install pgo-compare pgo-clean clean
