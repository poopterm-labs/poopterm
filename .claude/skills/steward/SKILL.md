---
name: steward
description: Use when driving an open Poopterm pull request to green - CI failures, the "Title and 3:1 test ratio" check, review comments, or merge conflicts with main.
---

# Stewarding a Poopterm pull request

## Conventions

- **Your own branch:** pick up `main` by rebasing
  (`git pull --rebase origin main`, then `git push --force-with-lease`),
  as AGENTS.md says. **Someone else's branch:** merge `main` in; never
  rewrite their history.
- `Cargo.lock` conflicts: take `main`'s, then `cargo build` to regenerate.
  Never resolve `CHANGELOG.md` release sections, `Cargo.toml` version or
  `Formula/` by hand in favor of the branch: `main`'s release commits win.
- Merging is squash only, and only once every required check is green.
- Be proactive on CI and review bots; ask before widening the pull request.

## Reproducing CI failures locally

| Failing job or step | Run locally |
|---------------------|-------------|
| `check` / Run fmt | `cargo fmt --check` (fix: `cargo fmt`) |
| `check` / Run tests | `cargo test --locked` |
| `check` / Run clippy | `cargo clippy --locked --all-targets -- -D warnings` |
| `check` / Check BSD-specific code | `rustup target add x86_64-unknown-freebsd && cargo clippy --locked --target x86_64-unknown-freebsd --all-targets -- -D warnings` |
| `check` (macos-latest) | `cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings`; read the job log for `#[cfg(target_os = "macos")]` code |
| Test release tools and PTY benchmark | `python3 -m unittest discover -s tests` |
| `freebsd` VM | Tests involving PTYs, shells and paths; check `cfg` branches and missing shells (tests must return early when a program is absent) |
| `alpine` | musl: the binary must stay dynamically linked (`.cargo/config.toml`) |
| `debian-package`, `fedora-package`, `arch-package` | `bash scripts/package-deb.sh`, `bash scripts/package-rpm.sh`; tests in `tests/test_package_*.py`, `tests/test_pkgbuild.py` |
| Title and 3:1 test ratio | `python3 scripts/pr_check.py --base origin/main --title "<PR title>"` |
| CodeQL | Read the alert; fix the code, never suppress it |

## The rules check

- **Title:** fix it on the pull request (`type(scope): summary`, lower
  case, no final period); the check re-runs on edit, no push needed.
- **Ratio:** add real tests. **REQUIRED SUB-SKILL:** meeting-the-test-ratio.
  Never pad, never move production code into test files to game it.

## Flaky tests

PTY, shell-integration and D-Bus tests touch real processes and timing.
"Flaky" is not a root cause: find the race (fixed sleeps, unread pipes,
ordering between threads) and make the test wait on a condition. Never
skip, `#[ignore]` or loosen a test to get green.

## Before every push

Run fmt, clippy, test, and pr_check with the PR's real title; cross-target
clippy when `cfg` code changed. CI runs only for branches with a pull
request, so the first red signal costs a full cycle.
