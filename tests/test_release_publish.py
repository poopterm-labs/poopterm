"""Exercise publication against local Git remotes and a fake GitHub API."""
import datetime
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).parents[1]


class Publish(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        # Like a CI runner, ignore personal Git settings such as tag.gpgsign.
        (self.root / 'gitconfig').write_text('')
        for name, value in [('GIT_CONFIG_GLOBAL', str(self.root / 'gitconfig')), ('GIT_CONFIG_NOSYSTEM', '1')]:
            self.addCleanup(lambda name=name, old=os.environ.get(name): os.environ.pop(name) if old is None else os.environ.__setitem__(name, old))
            os.environ[name] = value
        self.repo = self.root / 'work'
        self.remote = self.root / 'remote.git'
        self.repo.mkdir()
        self.git('init', '--bare', str(self.remote))
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Release test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('remote', 'add', 'origin', str(self.remote))
        (self.repo / 'scripts').mkdir()
        (self.repo / 'Formula').mkdir()
        for name in ['release.py', 'release.sh']:
            shutil.copy(SOURCE / 'scripts' / name, self.repo / 'scripts' / name)
        today = datetime.datetime.now(datetime.timezone.utc).date()
        self.version = f'{today.year % 100}.{today.month}.0'
        (self.repo / 'Cargo.toml').write_text(f'[package]\nname = "poopterm"\nversion = "{self.version}"\n[package.metadata.poopterm]\ncodename = "Giga Fly"\n')
        (self.repo / 'Cargo.lock').write_text(f'version = 4\n[[package]]\nname = "poopterm"\nversion = "{self.version}"\n')
        (self.repo / 'CHANGELOG.md').write_text('# Changelog\n\n## Old\n')
        (self.repo / 'Formula/poopterm.rb').write_text('  tag: "v0.43.0",\n  version "0.43.0"\n')
        # A package left over from before the bump, as CI artifacts used to be.
        dist = self.repo / 'dist'
        dist.mkdir()
        (dist / f'poopterm_{self.version}-1_amd64.deb').write_bytes(b'stale package')
        # Packages whatever version Cargo.toml holds, or FAKE_DEB_VERSION.
        package = self.repo / 'scripts/package-deb.sh'
        package.write_text('''#!/usr/bin/env bash
set -euo pipefail
version=${FAKE_DEB_VERSION:-$(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)}
rm -f dist/poopterm_*.deb
echo "package $version" >> "$FAKE_GH_ROOT/build.log"
echo built > "dist/poopterm_${version}-1_amd64.deb"
''')
        package.chmod(0o755)
        self.git('add', '.')
        self.git('commit', '-m', 'Initial checked revision')
        self.sha = self.git('rev-parse', 'HEAD').strip()
        self.git('push', 'origin', 'main')
        fakebin = self.root / 'bin'
        fakebin.mkdir()
        gh = fakebin / 'gh'
        gh.write_text('''#!/usr/bin/env python3
import os
from pathlib import Path
import sys
root = Path(os.environ['FAKE_GH_ROOT'])
with (root / 'api.log').open('a') as log:
    log.write(' '.join(sys.argv[1:]) + '\\n')
if (root / 'fail-api').exists():
    sys.stderr.write('HTTP 503 unavailable\\n')
    sys.exit(1)
if sys.argv[1] == 'api' and not (root / 'released').exists():
    sys.stderr.write('HTTP 404 Not Found\\n')
    sys.exit(1)
if sys.argv[1] == 'api':
    assets = (root / 'assets').read_text().split() if (root / 'assets').exists() else []
    print('{"assets": [' + ', '.join(f'{{"name": "{name}"}}' for name in assets) + ']}')
if sys.argv[1:3] == ['release', 'create']:
    (root / 'released').touch()
if sys.argv[1:3] == ['release', 'upload']:
    with (root / 'assets').open('a') as assets:
        assets.write(Path(sys.argv[4]).name + '\\n')
''')
        gh.chmod(0o755)
        cargo = fakebin / 'cargo'
        cargo.write_text('''#!/usr/bin/env bash
echo "cargo $* $(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)" >> "$FAKE_GH_ROOT/build.log"
[[ ! -e "$FAKE_GH_ROOT/fail-build" ]]
''')
        cargo.chmod(0o755)
        self.env = dict(os.environ, GITHUB_SHA=self.sha, GH_REPO='test/poopterm',
                        GH_TOKEN='fake-local-test-token', FAKE_GH_ROOT=str(self.root),
                        PATH=str(fakebin) + os.pathsep + os.environ['PATH'])

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, stderr=subprocess.PIPE, text=True)

    def publish(self):
        return subprocess.run(['bash', 'scripts/release.sh'], cwd=self.repo, env=self.env,
                              capture_output=True, text=True)

    def test_release_tags_bump_then_updates_formula_and_reruns_are_idempotent(self):
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertIn(f'version = "{version}"', self.git('show', f'v{version}:Cargo.toml'))
        self.assertIn(f'tag: "v{version}"', self.git('show', 'origin/main:Formula/poopterm.rb'))
        rerun = self.publish()
        self.assertEqual(rerun.returncode, 0)
        self.assertIn('already published', rerun.stdout)
        self.assertEqual((self.root / 'api.log').read_text().count('release create'), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release upload'), 1)
        self.assertEqual(len((self.root / 'build.log').read_text().splitlines()), 2)

    def test_a_release_without_its_package_gets_it_on_a_rerun(self):
        # Created, but the upload failed: the rerun builds and uploads only.
        (self.root / 'released').touch()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        log = (self.root / 'api.log').read_text()
        self.assertEqual(log.count('release create'), 0)
        self.assertEqual(log.count('release upload'), 1)

    def test_release_commits_do_not_start_another_ci_run(self):
        # The deploy key's pushes trigger workflows, unlike GITHUB_TOKEN's;
        # without the marker each release commit would release again.
        self.assertEqual(self.publish().returncode, 0)
        message = self.git('log', '-1', '--format=%B', 'origin/main')
        self.assertIn('[skip ci]', message, message)
        self.assertTrue(message.startswith('chore(release): '), message)

    def test_a_release_pushes_to_main_once_with_the_formula_in_the_tagged_commit(self):
        # A second push, after publishing, raced every merge made meanwhile.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.git('rev-parse', 'origin/main~1').strip(), self.sha)
        self.assertEqual(self.git('rev-parse', 'origin/main').strip(),
                         self.git('rev-parse', f'v{version}^{{commit}}').strip())
        formula = self.git('show', f'v{version}:Formula/poopterm.rb')
        self.assertIn(f'tag: "v{version}"', formula)
        self.assertIn(f'version "{version}"', formula)
        changed = self.git('show', '--name-only', '--format=', 'origin/main').split()
        self.assertEqual(sorted(changed), ['CHANGELOG.md', 'Cargo.lock', 'Cargo.toml', 'Formula/poopterm.rb'])

    def test_a_rerun_after_a_publishing_failure_pushes_nothing_more(self):
        failure = self.root / 'fail-api'
        failure.touch()
        self.assertNotEqual(self.publish().returncode, 0)
        pushed = self.git('ls-remote', str(self.remote))
        failure.unlink()
        self.assertEqual(self.publish().returncode, 0)
        self.assertEqual(self.git('ls-remote', str(self.remote)), pushed)

    def test_a_push_refused_for_another_reason_still_fails(self):
        hook = self.remote / 'hooks/pre-receive'
        hook.write_text('#!/bin/sh\necho refused by policy >&2\nexit 1\n')
        hook.chmod(0o755)
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('main moved', result.stdout)
        self.assertFalse((self.root / 'api.log').exists())

    def uploads(self):
        log = (self.root / 'api.log').read_text().splitlines()
        return [line.split()[-2] for line in log if line.startswith('release upload')]

    def test_the_package_is_built_from_the_bumped_release_commit(self):
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), [f'dist/poopterm_{version}-1_amd64.deb'])
        build = (self.root / 'build.log').read_text().splitlines()
        self.assertEqual(build, [f'cargo build --release --locked {version}', f'package {version}'])

    def test_a_package_of_another_version_is_never_uploaded(self):
        self.env['FAKE_DEB_VERSION'] = self.version
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('expected exactly one', result.stderr)
        self.assertEqual(self.uploads(), [])

    def test_a_failed_build_stops_before_the_github_release(self):
        (self.root / 'fail-build').touch()
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertNotIn('release create', (self.root / 'api.log').read_text())
        (self.root / 'fail-build').unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), [f'dist/poopterm_{version}-1_amd64.deb'])
        self.assertEqual(len(self.git('tag').splitlines()), 1)

    def test_moved_main_skips_every_release_mutation(self):
        (self.repo / 'new').touch()
        self.git('add', 'new')
        self.git('commit', '-m', 'New unchecked revision')
        self.git('push', 'origin', 'main')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('main moved', result.stdout)
        self.assertEqual(self.git('tag'), '')
        self.assertFalse((self.root / 'api.log').exists())
        self.assertFalse((self.root / 'build.log').exists())

    def test_api_failure_stops_and_rerun_resumes_without_another_bump(self):
        failure = self.root / 'fail-api'
        failure.touch()
        self.assertNotEqual(self.publish().returncode, 0)
        failure.unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(self.git('tag').splitlines()), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release create'), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release upload'), 1)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), [f'dist/poopterm_{version}-1_amd64.deb'])

    def test_a_push_race_cannot_publish_a_tag_or_overwrite_main(self):
        racer = self.root / 'racer'
        self.git('clone', '--branch', 'main', str(self.remote), str(racer))
        real_git = shutil.which('git')
        for args in [('config', 'user.name', 'Racer'), ('config', 'user.email', 'race@example.invalid')]:
            subprocess.check_call([real_git, '-C', str(racer), *args], stdout=subprocess.DEVNULL)
        (racer / 'racing-change').touch()
        subprocess.check_call([real_git, '-C', str(racer), 'add', '.'])
        subprocess.check_call([real_git, '-C', str(racer), 'commit', '-m', 'Race'], stdout=subprocess.DEVNULL)
        race_sha = subprocess.check_output([real_git, '-C', str(racer), 'rev-parse', 'HEAD'], text=True).strip()
        wrapper = self.root / 'bin/git'
        wrapper.write_text(f'''#!/usr/bin/env python3
import os
import subprocess
import sys
if '--atomic' in sys.argv:
    subprocess.check_call([{real_git!r}, '-C', {str(racer)!r}, 'push', 'origin', 'HEAD:main'])
os.execv({real_git!r}, [{real_git!r}, *sys.argv[1:]])
''')
        wrapper.chmod(0o755)
        # Losing the race is a skip, like a main that moved before the run:
        # the racing commit's own run releases it.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('main moved', result.stdout)
        actual = subprocess.check_output([real_git, '--git-dir', str(self.remote), 'rev-parse', 'main'], text=True).strip()
        self.assertEqual(actual, race_sha)
        self.assertEqual(subprocess.check_output([real_git, '--git-dir', str(self.remote), 'tag'], text=True), '')
        self.assertFalse((self.root / 'api.log').exists())
        self.assertFalse((self.root / 'build.log').exists())


if __name__ == '__main__':
    unittest.main()
