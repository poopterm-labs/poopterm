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
  git add Cargo.toml Cargo.lock CHANGELOG.md
  git commit -m "chore: release ${version} after green CI" -m "Poopterm-Release-For: ${GITHUB_SHA}"
  git tag "v${version}"
  # Atomic publication plus a lease closes the race between the fetch and push.
  # This is a fast-forward; the lease also rejects a newer main in the meantime.
  git push --atomic --force-with-lease="refs/heads/main:${GITHUB_SHA}" origin HEAD:main "refs/tags/v${version}"
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
# Create only when absent; API failures other than 404 must stop publication.
if ! gh api "repos/${GH_REPO}/releases/tags/v${version}" > /tmp/poopterm-release.json 2> /tmp/poopterm-release-error; then
  if ! grep -q 'HTTP 404' /tmp/poopterm-release-error; then
    cat /tmp/poopterm-release-error >&2
    exit 1
  fi
  python3 -c 'from pathlib import Path; s=Path("CHANGELOG.md").read_text(); print("## " + s.split("\n## ", 2)[1])' > /tmp/poopterm-release-notes.md
  gh release create "v${version}" --verify-tag --title "Poopterm ${version} \"${codename}\"" --notes-file /tmp/poopterm-release-notes.md
fi
# The tag now exists. Update Homebrew in a separate commit, also guarded.
release_commit=$(git rev-parse HEAD)
python3 scripts/release.py --formula "$version"
git add Formula/poopterm.rb
if ! git diff --cached --quiet; then
  git commit -m "chore: point Homebrew at published v${version}"
  git push --force-with-lease="refs/heads/main:${release_commit}" origin HEAD:main
fi
