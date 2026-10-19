"""Pin the parts of the CI workflow that other steps and people rely on."""
from pathlib import Path
import re
import unittest

WORKFLOW = (Path(__file__).parents[1] / '.github/workflows/ci.yml').read_text()


def step(name):
    """The lines of the workflow step called `name`, up to the next step or job."""
    lines = WORKFLOW.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.strip() == f'- name: {name}':
            indent = len(line) - len(line.lstrip())
            body = []
            for rest in lines[index + 1:]:
                if rest.strip() and len(rest) - len(rest.lstrip()) <= indent:
                    break
                body.append(rest)
            return ''.join(body)
    raise AssertionError(f'no step {name!r}')


class DebianPackage(unittest.TestCase):
    def test_the_package_uploads_as_the_deb_itself_not_a_zip(self):
        # People trying a pull request's package download and install it as is.
        upload = step('Upload Debian package')
        self.assertIn('actions/upload-artifact@v7', upload)
        self.assertRegex(upload, r'archive: false\n')
        self.assertRegex(upload, r'path: dist/\*\.deb\n')

    def test_the_direct_upload_has_no_name_to_download_it_by(self):
        # A direct upload is named after its file, so a step downloading
        # `poopterm-deb` would fail; releases build their own package instead.
        self.assertNotIn('name: poopterm-deb', WORKFLOW)
        self.assertNotIn('download-artifact', WORKFLOW)

    def test_a_missing_package_fails_the_job(self):
        self.assertRegex(step('Upload Debian package'), r'if-no-files-found: error\n')

    def test_the_step_lookup_rejects_unknown_steps(self):
        with self.assertRaises(AssertionError):
            step('No such step')

    def test_the_step_lookup_stops_at_the_next_step_or_job(self):
        self.assertNotIn('run:', step('Install nightly Rust').split('run:', 1)[1])
        self.assertNotIn('release:', step('Upload Debian package'))


def job(name):
    """The lines of the workflow job called `name`, up to the next job."""
    match = WORKFLOW.split(f'\n  {name}:\n', 1)
    if len(match) != 2:
        raise AssertionError(f'no job {name!r}')
    body = []
    for line in match[1].splitlines(keepends=True):
        if line.strip() and len(line) - len(line.lstrip()) <= 2:
            break
        body.append(line)
    return ''.join(body)


class Release(unittest.TestCase):
    def test_the_release_job_installs_a_python_with_tomllib(self):
        # Ubuntu 22.04, kept for the package's glibc floor, ships Python 3.10;
        # the release scripts read Cargo.toml with tomllib, new in 3.11.
        release = job('release')
        self.assertIn('runs-on: ubuntu-22.04', release)
        setup = step('Install Python for the release scripts')
        self.assertIn('actions/setup-python@v7', setup)
        version = re.search(r"python-version: '3\.(\d+)'", setup)
        self.assertIsNotNone(version, setup)
        self.assertGreaterEqual(int(version[1]), 11)

    def test_python_is_installed_before_the_release_scripts_run(self):
        release = job('release')
        self.assertIn('- name: Install Python for the release scripts', release)
        self.assertLess(release.index('- name: Install Python for the release scripts'),
                        release.index('- name: Publish the checked main revision'))

    def test_the_release_scripts_need_tomllib(self):
        # If they stop needing it, the setup step above can go.
        scripts = Path(__file__).parents[1] / 'scripts'
        self.assertIn('import tomllib', (scripts / 'release.py').read_text())
        self.assertIn('import tomllib', (scripts / 'release.sh').read_text())

    def test_the_job_lookup_stops_at_the_next_job(self):
        self.assertNotIn('release:', job('debian-package'))
        self.assertIn('scripts/release.sh', job('release'))
        with self.assertRaises(AssertionError):
            job('no-such-job')


if __name__ == '__main__':
    unittest.main()
