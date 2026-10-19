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


class Provenance(unittest.TestCase):
    def test_released_packages_get_a_signed_build_provenance(self):
        # `gh attestation verify` then proves which workflow run and commit
        # built a downloaded package.
        attest = step('Attest the released package')
        self.assertIn('uses: actions/attest-build-provenance@v4', attest)
        self.assertRegex(attest, r'subject-path: dist/poopterm_\*\.deb\n')

    def test_provenance_is_attested_after_the_release_built_the_package(self):
        release = job('release')
        self.assertLess(release.index('- name: Publish the checked main revision'),
                        release.index('- name: Attest the released package'))

    def test_runs_that_skip_the_release_attest_nothing(self):
        # A run that lost the race to a newer commit, or found the release
        # finished, builds no package.
        attest = step('Attest the released package')
        self.assertIn("if: hashFiles('dist/poopterm_*.deb') != ''", attest)

    def test_only_the_release_job_may_sign(self):
        release = job('release')
        for permission in ['contents: write', 'id-token: write', 'attestations: write']:
            self.assertIn(permission, release)
        top = WORKFLOW.split('\njobs:', 1)[0]
        self.assertIn('contents: read', top)
        self.assertNotIn('id-token', top)
        self.assertEqual(WORKFLOW.count('id-token: write'), 1)


class FreeBSD(unittest.TestCase):
    """The FreeBSD VM is the slowest job; it runs only what needs FreeBSD."""

    def vm(self):
        return step('Build and test on FreeBSD')

    def test_the_vm_runs_the_tests_and_the_binary(self):
        vm = self.vm()
        self.assertIn('cargo test --locked', vm)
        self.assertIn('cargo build --locked', vm)
        self.assertIn('./target/debug/poopterm --version', vm)

    def test_the_vm_skips_the_slow_release_build(self):
        # Fat LTO took half the job, and no FreeBSD binary ships.
        self.assertNotIn('--release', self.vm())

    def test_lints_and_formatting_run_on_linux_instead(self):
        vm = self.vm()
        self.assertNotIn('cargo fmt', vm)
        self.assertNotIn('cargo clippy', vm)
        self.assertNotIn('--component clippy', vm)
        bsd = step('Check BSD-specific code')
        self.assertIn('cargo +nightly clippy --locked --target x86_64-unknown-freebsd --all-targets -- -D warnings', bsd)

    def test_the_vm_installs_libraries_not_the_whole_x_server(self):
        prepare = self.vm().split('prepare:', 1)[1].split('run:', 1)[0]
        packages = prepare.split('pkg install -y', 1)[1].split('\n', 1)[0].split()
        self.assertNotIn('xorg', packages)
        for package in ['pkgconf', 'libX11', 'libxkbcommon', 'wayland', 'bash', 'zsh']:
            self.assertIn(package, packages)


class Speed(unittest.TestCase):
    def test_the_linux_check_leaves_release_builds_to_the_package_job(self):
        check = job('check')
        build = step('Build release binary')
        self.assertIn("if: runner.os == 'macOS'", build)
        self.assertIn('cargo +nightly build --release --locked', job('debian-package'))
        self.assertIn("if: runner.os == 'macOS'", step('Verify binary execution'))
        self.assertIn('Build release binary', check)

    def test_a_new_push_to_a_pull_request_cancels_its_older_run(self):
        self.assertIn("concurrency:\n  group: ci-${{ github.event_name == 'pull_request' && github.ref || github.sha }}\n", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)

    def test_runs_on_main_are_never_cancelled(self):
        # Cancelling one could stop a release halfway through publishing; a
        # group per commit also keeps pushes to main from queueing (a group
        # holds one running and one pending run, and drops older pending ones).
        self.assertNotIn('cancel-in-progress: true', WORKFLOW)
        self.assertIn('|| github.sha }}', WORKFLOW)
        release = job('release')
        self.assertIn('group: poopterm-release', release)
        self.assertIn('cancel-in-progress: false', release)


if __name__ == '__main__':
    unittest.main()
