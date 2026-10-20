#!/usr/bin/env bash
# Publish only the main commit that passed every required CI job.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${GITHUB_SHA:?}" "${GH_REPO:?}" "${GH_TOKEN:?}"
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git fetch origin main --tags
remote=$(git rev-parse origin/main)
if [[ "$remote" == "$GITHUB_SHA" ]]; then
  release=$(python3 scripts/release.py)
  version=${release%%$'\n'*}
  codename=${release#*$'\n'}
  # Homebrew builds from the tag, which lands with this commit, so the
  # formula goes in it too: a release pushes to main exactly once.
  python3 scripts/release.py --formula "$version"
  git add Cargo.toml Cargo.lock CHANGELOG.md Formula/poopterm.rb
  # Pushed with the deploy key, which triggers workflows: [skip ci] stops
  # this commit from releasing again.
  git commit -m "chore(release): ${version} after green CI" -m "[skip ci]" -m "Poopterm-Release-For: ${GITHUB_SHA}"
  git tag "v${version}"
  # Atomic publication plus a lease closes the race between the fetch and push.
  # This is a fast-forward; the lease also rejects a newer main in the meantime.
  if ! git push --atomic --force-with-lease="refs/heads/main:${GITHUB_SHA}" origin HEAD:main "refs/tags/v${version}"; then
    # Losing the race to a merge is a skip: that commit's own run releases.
    git fetch origin main
    if [[ "$(git rev-parse origin/main)" != "$GITHUB_SHA" ]]; then
      echo 'main moved while releasing; skipping release.'
      exit 0
    fi
    exit 1
  fi
elif [[ "$(git log -1 --format=%B origin/main)" == *"Poopterm-Release-For: ${GITHUB_SHA}"* ]]; then
  # A rerun after a publishing failure resumes this release, without another bump.
  git checkout --detach origin/main
  version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
  codename=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["metadata"]["poopterm"]["codename"])')
  [[ "$(git rev-parse "v${version}")" == "$(git rev-parse HEAD)" ]]
else
  echo 'main moved since this CI run; skipping release.'
  exit 0
fi
# Look the release up first, drafts included (the tag endpoint omits them);
# API failures must stop publication. State is none, draft or published,
# then whether this version's package is attached.
gh api "repos/${GH_REPO}/releases?per_page=100" > /tmp/poopterm-releases.json
read -r state attached < <(python3 - "v${version}" "poopterm_${version}-1_" <<'PY'
import json, sys
tag, package = sys.argv[1:]
release = next((r for r in json.load(open('/tmp/poopterm-releases.json')) if r['tag_name'] == tag), None)
if release is None:
    print('none no')
else:
    has = any(a['name'].startswith(package) for a in release['assets'])
    print('draft' if release['draft'] else 'published', 'yes' if has else 'no')
PY
)
# A rerun of a finished release has nothing left to do.
if [[ $state == published && $attached == yes ]]; then
  echo "v${version} is already published."
  exit 0
fi
if [[ $attached == no ]]; then
  # Package the release commit itself, so the .deb and the binary in it carry
  # the new version; a failed build stops before the GitHub release exists.
  # Releases ship the PGO build, bolted on Linux when llvm-bolt is
  # installed, or a plain one if training fails.
  bash scripts/pgo.sh dist
  bash scripts/package-deb.sh
  shopt -s nullglob
  packages=(dist/poopterm_"${version}"-1_*.deb)
  if ((${#packages[@]} != 1)); then
    echo "expected exactly one Debian package of ${version} in dist/, found ${#packages[@]}" >&2
    exit 1
  fi
fi
# Releases start as drafts and are published once the package is attached,
# since immutable releases refuse new assets after publication.
if [[ $state == none ]]; then
  python3 -c 'from pathlib import Path; s=Path("CHANGELOG.md").read_text(); print("## " + s.split("\n## ", 2)[1])' > /tmp/poopterm-release-notes.md
  gh release create "v${version}" --draft --verify-tag --title "Poopterm ${version} \"${codename}\"" --notes-file /tmp/poopterm-release-notes.md
fi
# The APT feed polls this release asset and republishes it from GitHub Pages.
if [[ $attached == no ]]; then
  gh release upload "v${version}" "${packages[0]}" --clobber
fi
if [[ $state != published ]]; then
  gh release edit "v${version}" --draft=false
fi
