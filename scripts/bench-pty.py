#!/usr/bin/env python3
"""Measure Python + kernel PTY throughput with a reader that only discards.

Uses the same payloads and 64 KiB writes as bench-throughput.py. The slave
is raw: no newline expansion or flow control, so this is an upper bound.
Timing includes draining every byte on a separate reader thread, but excludes
PTY creation, payload generation, and thread startup. No window is needed.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import statistics
import threading
import time
import tty


def measure(data):
    master, slave = os.openpty()
    tty.setraw(slave)
    ready = threading.Event()
    done = threading.Event()
    received = 0
    errors = []

    def discard():
        nonlocal received
        ready.set()
        try:
            while received < len(data):
                chunk = os.read(master, 1 << 16)
                if not chunk:
                    raise EOFError('PTY closed before all bytes were read')
                received += len(chunk)
        except BaseException as error:
            errors.append(error)
        finally:
            done.set()

    reader = threading.Thread(target=discard, daemon=True)
    reader.start()
    ready.wait()
    try:
        start = time.perf_counter()
        view = memoryview(data)
        while view:
            count = os.write(slave, view[:1 << 16])
            view = view[count:]
        if not done.wait(120):
            raise TimeoutError('PTY reader did not drain the payload')
        seconds = time.perf_counter() - start
        if errors:
            raise errors[0]
        return seconds, received
    finally:
        os.close(slave)
        os.close(master)
        reader.join(timeout=1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mib', type=int, default=32)
    parser.add_argument('--repeat', type=int, default=3)
    parser.add_argument('--format', choices=['text', 'json'], default='text')
    args = parser.parse_args()
    if args.mib < 1 or args.repeat < 1:
        parser.error('--mib and --repeat must be positive')
    spec = importlib.util.spec_from_file_location('throughput', Path(__file__).with_name('bench-throughput.py'))
    throughput = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(throughput)
    results = []
    for key, name, data in throughput.build_payloads(args.mib << 20):
        measure(data[:len(data) // 8])
        rates = [len(data) / measure(data)[0] / 1e6 for _ in range(args.repeat)]
        results.append(dict(payload=key, median_mb_s=statistics.median(rates),
                            min_mb_s=min(rates), max_mb_s=max(rates)))
    if args.format == 'json':
        print(json.dumps(dict(consumer='discard', raw=True, repeat=args.repeat, results=results), indent=2))
    else:
        for result in results:
            print(f"{result['payload']:<8} {result['median_mb_s']:8.1f} MB/s "
                  f"({result['min_mb_s']:.1f}-{result['max_mb_s']:.1f})")


if __name__ == '__main__':
    main()
