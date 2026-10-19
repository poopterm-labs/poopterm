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
# Look the release up first; API failures other than 404 must stop publication.
released=yes
if ! gh api "repos/${GH_REPO}/releases/tags/v${version}" > /tmp/poopterm-release.json 2> /tmp/poopterm-release-error; then
  if ! grep -q 'HTTP 404' /tmp/poopterm-release-error; then
    cat /tmp/poopterm-release-error >&2
    exit 1
  fi
  released=no
fi
# A rerun of a finished release has nothing left to do.
if [[ $released == yes ]] && grep -q "\"poopterm_${version}-1_" /tmp/poopterm-release.json; then
  echo "v${version} is already published."
  exit 0
fi
# Package the release commit itself, so the .deb and the binary in it carry
# the new version; a failed build stops before the GitHub release exists.
cargo build --release --locked
bash scripts/package-deb.sh
if [[ $released == no ]]; then
  python3 -c 'from pathlib import Path; s=Path("CHANGELOG.md").read_text(); print("## " + s.split("\n## ", 2)[1])' > /tmp/poopterm-release-notes.md
  gh release create "v${version}" --verify-tag --title "Poopterm ${version} \"${codename}\"" --notes-file /tmp/poopterm-release-notes.md
fi
# The APT feed polls this release asset and republishes it from GitHub Pages.
shopt -s nullglob
packages=(dist/poopterm_"${version}"-1_*.deb)
if ((${#packages[@]} != 1)); then
  echo "expected exactly one Debian package of ${version} in dist/, found ${#packages[@]}" >&2
  exit 1
fi
gh release upload "v${version}" "${packages[0]}" --clobber
