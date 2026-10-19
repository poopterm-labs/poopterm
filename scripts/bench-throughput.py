#!/usr/bin/env python3
"""Terminal-agnostic output throughput benchmark over real-world workloads.

Measures how fast the terminal *running this script* consumes output, in the
spirit of kitty's `kitten __benchmark__`, but using nothing terminal-specific:

* Workloads come from poopterm's corpus (`poopterm --bench-corpus`): build
  logs, colored compiler output, ls, highlighted source, Unicode, CJK, emoji,
  full-screen TUI redraws, editor scrolling, progress bars, truecolor art,
  shell prompts with OSC 133, Sixel and kitty graphics. They are
  deterministic, so every terminal gets identical bytes. Build poopterm
  first (`cargo build --release`), or point POOPTERM_BIN at a binary.
* Each measurement ends with a Primary Device Attributes query (`CSI c`),
  which every VT100-compatible terminal answers only after parsing all prior
  output, so buffering inside the terminal cannot inflate the numbers.
* One warm-up pass, then `--repeat` timed passes; the median is reported
  along with min/max so noisy runs are visible.
* Workloads are generated for the window's size, which is recorded, because
  rendering cost scales with the grid. Compare terminals at the same size.

Usage (run inside the terminal under test):

    python3 scripts/bench-throughput.py                    # 16 MiB x 3 per workload
    python3 scripts/bench-throughput.py --mib 64 --repeat 5 --only log --only tui
    python3 scripts/bench-throughput.py --output results.json --format json
"""
import argparse
from collections import namedtuple
import json
import os
from pathlib import Path
import re
import select
import shutil
import statistics
import subprocess
import sys
import termios
import time
import tty

CHUNK = 1 << 16  # bytes per write(2) call, identical for every terminal
ROOT = Path(__file__).resolve().parents[1]

# `data` is the timed payload; `warm_up` is about an eighth of it. Both end
# on whole bursts, so no escape sequence or UTF-8 character is cut: a DA1
# query sent after a cut sequence is swallowed by some terminals (kitty).
Payload = namedtuple("Payload", "key name data warm_up")


class NoReply(Exception):
    pass


def wait_for_terminal(fd, timeout, query=b""):
    """Send `query` and DA1, and block until the DA1 reply (`ESC [ ? ... c`)
    arrives. Returns everything the terminal answered."""
    old = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        os.write(sys.stdout.fileno(), query + b"\x1b[c")
        reply = b""
        deadline = time.monotonic() + timeout
        while not (b"\x1b[" in reply and reply.endswith(b"c")):
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
                raise NoReply("terminal did not answer the DA1 query (CSI c)")
            reply += os.read(fd, 256)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
    return reply


def parse_xtversion(reply):
    """The name in an XTVERSION reply (`DCS > | name ST`), if any."""
    match = re.search(rb"\x1bP>\|(.*?)\x1b\\", reply, re.S)
    return match.group(1).decode(errors="replace") if match else None


def poopterm_binary():
    binary = Path(os.environ.get("POOPTERM_BIN") or ROOT / "target/release/poopterm")
    if not binary.is_file():
        raise FileNotFoundError(f"{binary} not found; run `cargo build --release` or set POOPTERM_BIN")
    return binary


def workloads(binary):
    """(name, description) of every corpus workload."""
    listing = subprocess.run([binary, "--benchmark", "--list"], capture_output=True, text=True, check=True)
    return [tuple(line.split(None, 1)) for line in listing.stdout.splitlines() if line.strip()]


def corpus(binary, name, kib, columns, lines):
    return subprocess.run([binary, "--bench-corpus", name, "--kib", str(kib), "--size", f"{columns}x{lines}"],
                          capture_output=True, check=True).stdout


def build_payloads(mib, columns=120, lines=40, only=None, binary=None):
    binary = binary or poopterm_binary()
    payloads = []
    for key, name in workloads(binary):
        if only and key not in only:
            continue
        data = corpus(binary, key, mib << 10, columns, lines)
        # A smaller build is a prefix of the larger one: same seed, whole bursts.
        warm_up = corpus(binary, key, max(1, mib << 7), columns, lines)
        payloads.append(Payload(key, name, data, warm_up))
    return payloads


def write_all(out, data):
    view = memoryview(data)
    while view:
        view = view[os.write(out, view[:CHUNK]):]


def measure(fd, out, data, timeout):
    wait_for_terminal(fd, timeout)
    start = time.perf_counter()
    write_all(out, data)
    wait_for_terminal(fd, timeout)
    return time.perf_counter() - start


def main():
    parser = argparse.ArgumentParser(description="Terminal-agnostic throughput benchmark.")
    parser.add_argument("--mib", type=int, default=16, help="payload size per workload (MiB)")
    parser.add_argument("--repeat", type=int, default=3, help="timed passes per workload")
    parser.add_argument("--only", action="append", help="run only these workloads (repeatable)")
    parser.add_argument("--timeout", type=float, default=120.0,
                        help="seconds to wait for each DA1 reply")
    parser.add_argument("--output", help="also append results to this file")
    parser.add_argument("--format", choices=["text", "json"], default="text")
    args = parser.parse_args()
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        sys.exit("bench-throughput: stdin and stdout must be the terminal under test")

    fd, out = sys.stdin.fileno(), sys.stdout.fileno()
    size = shutil.get_terminal_size()
    try:
        payloads = build_payloads(args.mib, size.columns, size.lines, args.only)
    except (OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"bench-throughput: {error}")
    if args.only and len(payloads) != len(set(args.only)):
        sys.exit("bench-throughput: unknown workload; see `poopterm --benchmark --list`")
    results = []
    try:
        # Environment variables such as TERM_PROGRAM may come from a parent
        # terminal; the terminal's own XTVERSION answer is authoritative.
        terminal = parse_xtversion(wait_for_terminal(fd, args.timeout, b"\x1b[>q"))
        for key, name, data, warm_up in payloads:
            measure(fd, out, warm_up, args.timeout)
            seconds = [measure(fd, out, data, args.timeout) for _ in range(args.repeat)]
            rates = [len(data) / s / 1e6 for s in seconds]
            results.append({
                "payload": key, "name": name, "bytes": len(data),
                "median_mb_s": statistics.median(rates), "min_mb_s": min(rates),
                "max_mb_s": max(rates),
            })
    except NoReply as error:
        sys.exit(f"bench-throughput: {error}")
    finally:
        # Also delete kitty images, which survive a screen clear.
        os.write(out, b"\x1b_Ga=d,q=2\x1b\\\x1b[0m\x1b[r\x1b[2J\x1b[H")

    total_bytes = sum(r["bytes"] for r in results)
    total_seconds = sum(r["bytes"] / r["median_mb_s"] / 1e6 for r in results)
    report = {
        "terminal": terminal or "", "term": os.environ.get("TERM", ""),
        "term_program": os.environ.get("TERM_PROGRAM", ""),
        "columns": size.columns, "lines": size.lines, "repeat": args.repeat, "results": results,
        "overall_mb_s": total_bytes / total_seconds / 1e6 if results else 0,
    }
    if args.format == "json":
        text = json.dumps(report, indent=2)
    else:
        header = (f"{terminal or report['term_program'] or report['term']}  {size.columns}x{size.lines}  "
                  f"{args.mib} MiB x {args.repeat} (median, min-max)")
        rows = [f"{r['payload']:<11} {r['median_mb_s']:9.1f} MB/s  "
                f"({r['min_mb_s']:.1f}-{r['max_mb_s']:.1f})  {r['name']}" for r in results]
        text = "\n".join([header, *rows, f"{'overall':<11} {report['overall_mb_s']:9.1f} MB/s"])
    print(text)
    if args.output:
        with open(args.output, "a") as handle:
            handle.write(text + "\n")


if __name__ == "__main__":
    main()
