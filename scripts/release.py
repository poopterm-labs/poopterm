#!/usr/bin/env python3
"""Prepare coordinated calendar-version releases; publishing lives in release.sh."""
import datetime
from pathlib import Path
import re
import secrets
import tomllib


def next_version(current, today):
    year, month, patch = map(int, current.split('.'))
    period = (today.year % 100, today.month)
    if period < (year, month):
        raise ValueError('refusing to release a version older than the current month')
    return f'{period[0]}.{period[1]}.{patch + 1 if period == (year, month) else 0}'


def prepare(root, today):
    manifest = (root / 'Cargo.toml').read_text()
    package = tomllib.loads(manifest)['package']
    old = package['version']
    new = next_version(old, today)
    codename = package['metadata']['poopterm']['codename']
    if old.rsplit('.', 1)[0] != new.rsplit('.', 1)[0]:
        previous = codename
        while codename == previous:
            codename = f"{secrets.choice(['Cosmic', 'Disco', 'Turbo', 'Velvet', 'Lunar', 'Electric'])} {secrets.choice(['Beetle', 'Moth', 'Gecko', 'Wombat', 'Otter', 'Fly'])}"
    manifest, count = re.subn(r'(?m)^(version\s*=\s*)"' + re.escape(old) + r'"',
                              lambda m: f'{m[1]}"{new}"', manifest, count=1)
    if count != 1:
        raise ValueError('package version was not found')
    manifest, count = re.subn(r'(?m)^codename\s*=.*$', f'codename = "{codename}"', manifest, count=1)
    if count != 1:
        raise ValueError('codename was not found')
    lock = (root / 'Cargo.lock').read_text()
    lock, count = re.subn(r'(\[\[package\]\]\nname = "poopterm"\nversion = )"' + re.escape(old) + r'"',
                          lambda m: f'{m[1]}"{new}"', lock)
    if count != 1:
        raise ValueError('exactly one poopterm lockfile entry is required')
    changelog = (root / 'CHANGELOG.md').read_text()
    heading, separator, history = changelog.partition('\n')
    notes = f'## {new} "{codename}" — {today.isoformat()}\n\n- Includes the changes verified by CI on main.\n\n'
    (root / 'Cargo.toml').write_text(manifest)
    (root / 'Cargo.lock').write_text(lock)
    (root / 'CHANGELOG.md').write_text(heading + '\n\n' + notes + history.lstrip('\n'))
    return new, codename


def update_formula(root, version):
    path = root / 'Formula/poopterm.rb'
    formula = path.read_text()
    formula, tags = re.subn(r'tag: "v[^"]+"', f'tag: "v{version}"', formula)
    formula, versions = re.subn(r'(?m)^  version "[^"]+"', f'  version "{version}"', formula)
    if tags != 1 or versions != 1:
        raise ValueError('expected one formula tag and version')
    path.write_text(formula)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--formula', metavar='VERSION')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.formula:
        update_formula(root, args.formula)
    else:
        version, codename = prepare(root, datetime.datetime.now(datetime.timezone.utc).date())
        print(f'{version}\n{codename}')
