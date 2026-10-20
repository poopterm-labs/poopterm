#!/bin/sh
# Profile-guided optimization: builds poopterm instrumented, trains it, and
# rebuilds it optimized for what the training ran.
#
#   scripts/pgo.sh                 # instrument, train on the benchmark, build
#   scripts/pgo.sh instrument      # instrumented build only
#   scripts/pgo.sh train [MIB]     # run the benchmark corpus (default 8 MiB each),
#                                  # as floods and as small interactive writes
#   scripts/pgo.sh use             # open the instrumented terminal and use it;
#                                  # closing it saves what you did as training
#   scripts/pgo.sh build           # merge every profile and build optimized
#   scripts/pgo.sh compare [MIB]   # benchmark the release and PGO builds
#   scripts/pgo.sh path            # print the optimized binary's path
#   scripts/pgo.sh dist [MIB]      # instrument, train and build, then put the
#                                  # result at target/release/poopterm; if any
#                                  # step fails, a plain release build instead
#   scripts/pgo.sh clean           # forget all profiles
#
# Profiles add up: train on the benchmark, add sessions with `use`, then
# `build` again. Old profiles still apply after code changes (functions that
# changed are left unoptimized), but retrain after large ones.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
host=$(rustc -vV | sed -n 's/^host: //p')
pgo=$root/target/pgo
profiles=$pgo/profiles
merged=$pgo/merged.profdata
# Separate target directories, so the instrumented and optimized builds do
# not rebuild each other, nor the plain release build.
instrumented=$pgo/instrumented/$host/release/poopterm
optimized=$pgo/optimized/$host/release/poopterm

# llvm-profdata must match rustc's LLVM, so use the toolchain's own.
profdata() {
	tool=$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata
	if [ ! -x "$tool" ]; then
		echo "pgo.sh: adding the llvm-tools component for llvm-profdata" >&2
		rustup component add llvm-tools >&2
	fi
	echo "$tool"
}

# Builds for the host target explicitly, so the flags reach poopterm and
# its dependencies but not build scripts. Existing RUSTFLAGS are kept.
cargo_build() {
	target_dir=$1
	shift
	RUSTFLAGS="${RUSTFLAGS:-} $*" cargo build --release --locked --target "$host" \
		--target-dir "$target_dir"
}

instrument() {
	mkdir -p "$profiles"
	cargo_build "$pgo/instrumented" "-Cprofile-generate=$profiles"
}

need_instrumented() {
	[ -x "$instrumented" ] || instrument
}

train() {
	need_instrumented
	"$instrumented" --benchmark --mib "${1:-8}"
	# Interactive programs write a few bytes at a time (key echo, prompts)
	# and get a frame for most writes; keep their paths in the profile too.
	"$instrumented" --benchmark --kib 64 --read 16 --frame-every 64
}

# The binary releases ship, at the path packaging expects. Training runs
# in its own process, so a failure anywhere in it falls back.
dist() {
	release=$root/target/release/poopterm
	if sh "$root/scripts/pgo.sh" all "$@"; then
		mkdir -p "$(dirname "$release")"
		cp "$optimized" "$release"
		echo "pgo.sh: shipping the PGO build"
	else
		echo "::warning::pgo.sh: PGO failed; shipping a plain release build" >&2
		cargo build --release --locked
	fi
}

use_terminal() {
	need_instrumented
	echo "Use this terminal as usual; its profile is saved when you close it."
	"$instrumented"
}

build() {
	if ! ls "$profiles"/*.profraw >/dev/null 2>&1; then
		echo "pgo.sh: no profiles yet; run 'scripts/pgo.sh train' first" >&2
		exit 1
	fi
	"$(profdata)" merge -o "$merged" "$profiles"/*.profraw
	cargo_build "$pgo/optimized" "-Cprofile-use=$merged"
	echo "Optimized build: $optimized"
}

compare() {
	[ -x "$optimized" ] || {
		echo "pgo.sh: no optimized build; run 'scripts/pgo.sh build' first" >&2
		exit 1
	}
	cargo build --release --locked
	for binary in "$root/target/release/poopterm" "$optimized"; do
		echo "== $binary"
		"$binary" --benchmark --mib "${1:-16}"
	done
}

command=${1:-all}
[ $# -gt 0 ] && shift
case $command in
all)
	instrument
	train "$@"
	build
	;;
instrument) instrument ;;
train) train "$@" ;;
use) use_terminal ;;
build) build ;;
compare) compare "$@" ;;
path) echo "$optimized" ;;
dist) dist "$@" ;;
clean) rm -rf "$profiles" "$merged" ;;
-h | --help) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//' ;;
*)
	echo "pgo.sh: unknown command: $command (try --help)" >&2
	exit 2
	;;
esac
