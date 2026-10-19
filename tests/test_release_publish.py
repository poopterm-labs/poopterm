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
        dist = self.repo / 'dist'
        dist.mkdir()
        (dist / f'poopterm_{self.version}-1_amd64.deb').write_bytes(b'test package')
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
if sys.argv[1:3] == ['release', 'create']:
    (root / 'released').touch()
''')
        gh.chmod(0o755)
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
        self.assertIn('v0.43.0', self.git('show', f'v{version}:Formula/poopterm.rb'))
        self.assertEqual(self.publish().returncode, 0)
        self.assertEqual((self.root / 'api.log').read_text().count('release create'), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release upload'), 1)

    def test_release_commits_do_not_start_another_ci_run(self):
        # The deploy key's pushes trigger workflows, unlike GITHUB_TOKEN's;
        # without the marker each release commit would release again.
        self.assertEqual(self.publish().returncode, 0)
        for commit in ['origin/main', 'origin/main~1']:
            message = self.git('log', '-1', '--format=%B', commit)
            self.assertIn('[skip ci]', message, message)
            self.assertTrue(message.startswith('chore('), message)

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
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        actual = subprocess.check_output([real_git, '--git-dir', str(self.remote), 'rev-parse', 'main'], text=True).strip()
        self.assertEqual(actual, race_sha)
        self.assertEqual(subprocess.check_output([real_git, '--git-dir', str(self.remote), 'tag'], text=True), '')
        self.assertFalse((self.root / 'api.log').exists())


if __name__ == '__main__':
    unittest.main()
