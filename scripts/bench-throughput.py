#!/usr/bin/env python3
"""Terminal-agnostic output throughput benchmark.

Measures how fast the terminal *running this script* consumes output, in the
spirit of kitty's `kitten __benchmark__`, but using nothing terminal-specific:

* Payloads are deterministic (fixed seed) and generated before timing starts.
* Each measurement ends with a Primary Device Attributes query (`CSI c`),
  which every VT100-compatible terminal answers only after parsing all prior
  output, so buffering inside the terminal cannot inflate the numbers.
* One warm-up pass, then `--repeat` timed passes; the median is reported
  along with min/max so noisy runs are visible.
* Window size, $TERM and payload sizes are recorded, because rendering cost
  scales with the grid size. Compare terminals at the same window size.

Usage (run inside the terminal under test):

    python3 scripts/bench-throughput.py                  # 32 MiB x 3 repeats
    python3 scripts/bench-throughput.py --mib 64 --repeat 5
    python3 scripts/bench-throughput.py --output results.json --format json
"""
import argparse
import json
import os
import random
import select
import shutil
import statistics
import sys
import termios
import time
import tty

CHUNK = 1 << 16  # bytes per write(2) call, identical for every terminal


class NoReply(Exception):
    pass


def wait_for_terminal(fd, timeout):
    """Send DA1 and block until the reply (`ESC [ ? ... c`) arrives."""
    old = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        os.write(sys.stdout.fileno(), b"\x1b[c")
        reply = b""
        deadline = time.monotonic() + timeout
        while not (b"\x1b[" in reply and reply.endswith(b"c")):
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
                raise NoReply("terminal did not answer the DA1 query (CSI c)")
            reply += os.read(fd, 256)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)


def build_payloads(size):
    rng = random.Random(1234)
    printable = bytes(range(0x20, 0x7F))
    ascii_chunk = b"".join(
        bytes(rng.choice(printable) for _ in range(rng.randint(40, 120))) + b"\n"
        for _ in range(512)
    )
    words = ["héllo", "wörld", "日本語", "terminal", "→", "λ", "🙂", "naïve", "box─┼─"]
    unicode_chunk = (
        "\n".join(" ".join(rng.choice(words) for _ in range(12)) for _ in range(256)) + "\n"
    ).encode()
    csi_chunk = b"".join(
        b"\x1b[%d;%dm%s\x1b[0m\x1b[38;2;%d;%d;%dmx" % (
            rng.randint(30, 37), rng.randint(40, 47), b"ab",
            rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255))
        for _ in range(2048)
    ) + b"\n"
    cursor_chunk = b"".join(
        b"\x1b[%d;%dHtext\x1b[K\x1b[2A\x1b[3C" % (rng.randint(1, 24), rng.randint(1, 80))
        for _ in range(2048)
    )
    chunks = [
        ("ascii", "Only ASCII chars", ascii_chunk),
        ("unicode", "Unicode chars", unicode_chunk),
        ("csi", "CSI codes with few chars", csi_chunk),
        ("cursor", "Cursor movement", cursor_chunk),
    ]
    return [(key, name, chunk * max(1, size // len(chunk))) for key, name, chunk in chunks]


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
    parser.add_argument("--mib", type=int, default=32, help="payload size per test (MiB)")
    parser.add_argument("--repeat", type=int, default=3, help="timed passes per payload")
    parser.add_argument("--only", choices=["ascii", "unicode", "csi", "cursor"], action="append",
                        help="run only these payloads (repeatable)")
    parser.add_argument("--timeout", type=float, default=120.0,
                        help="seconds to wait for each DA1 reply")
    parser.add_argument("--output", help="also append results to this file")
    parser.add_argument("--format", choices=["text", "json"], default="text")
    args = parser.parse_args()
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        sys.exit("bench-throughput: stdin and stdout must be the terminal under test")

    fd, out = sys.stdin.fileno(), sys.stdout.fileno()
    size = shutil.get_terminal_size()
    payloads = [p for p in build_payloads(args.mib * 1024 * 1024)
                if not args.only or p[0] in args.only]
    results = []
    try:
        for key, name, data in payloads:
            measure(fd, out, data[: len(data) // 8], args.timeout)  # warm-up
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
        os.write(out, b"\x1b[0m\x1b[2J\x1b[H")

    report = {
        "term": os.environ.get("TERM", ""), "term_program": os.environ.get("TERM_PROGRAM", ""),
        "columns": size.columns, "lines": size.lines, "repeat": args.repeat, "results": results,
    }
    if args.format == "json":
        text = json.dumps(report, indent=2)
    else:
        header = (f"{report['term_program'] or report['term']}  {size.columns}x{size.lines}  "
                  f"{args.mib} MiB x {args.repeat} (median, min-max)")
        rows = [f"{r['name']:<26} {r['median_mb_s']:9.1f} MB/s  "
                f"({r['min_mb_s']:.1f}-{r['max_mb_s']:.1f})" for r in results]
        text = "\n".join([header, *rows])
    print(text)
    if args.output:
        with open(args.output, "a") as handle:
            handle.write(text + "\n")


if __name__ == "__main__":
    main()
