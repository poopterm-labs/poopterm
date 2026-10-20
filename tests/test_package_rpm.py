"""The RPM package: version, file list, dependencies, and the untouched binary."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).parents[1]
SCRIPT = ROOT / 'scripts/package-rpm.sh'


@unittest.skipUnless(shutil.which('rpmbuild') and shutil.which('tic'), 'needs rpmbuild and tic')
class Package(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = Path(tempfile.mkdtemp())
        # A stand-in binary: package-rpm.sh reads the version from it.
        cls.binary = cls.temp / 'poopterm'
        cls.binary.write_text('#!/bin/sh\necho "poopterm 26.10.7"\n')
        cls.binary.chmod(0o755)
        cls.result = subprocess.run(['bash', str(SCRIPT), str(cls.binary)], cwd=ROOT,
                                    capture_output=True, text=True)
        cls.package = ROOT / f'dist/poopterm-26.10.7-1.{subprocess.check_output(["uname", "-m"], text=True).strip()}.rpm'

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.temp)
        cls.package.unlink(missing_ok=True)

    def query(self, *args):
        return subprocess.check_output(['rpm', '-qp', *args, str(self.package)], text=True,
                                       stderr=subprocess.DEVNULL)

    def test_the_package_is_named_after_the_binarys_version(self):
        self.assertEqual(self.result.returncode, 0, self.result.stdout + self.result.stderr)
        self.assertTrue(self.package.is_file(), list((ROOT / 'dist').iterdir()))
        self.assertEqual(self.query('--queryformat', '%{NAME} %{VERSION} %{RELEASE}'), 'poopterm 26.10.7 1')

    def test_it_installs_what_install_sh_does_under_usr(self):
        files = self.query('--list').split()
        for path in ['/usr/bin/poopterm', '/usr/share/applications/poopterm.desktop',
                     '/usr/share/icons/hicolor/16x16/apps/poopterm.png',
                     '/usr/share/icons/hicolor/512x512/apps/poopterm.png']:
            self.assertIn(path, files)
        self.assertTrue(any(f.startswith('/usr/share/terminfo/') and f.endswith('/xterm-poopterm') for f in files), files)
        self.assertFalse([f for f in files if f.startswith('/usr/local')], files)

    def test_it_owns_no_shared_directories_or_build_id_links(self):
        # Owning /usr/bin or a .build-id link would conflict with other packages.
        files = self.query('--list').split()
        self.assertNotIn('/usr/bin', files)
        self.assertNotIn('/usr/share/terminfo', files)
        self.assertFalse([f for f in files if '/.build-id' in f], files)

    def test_the_license_and_notices_are_marked_as_such(self):
        self.assertIn('/usr/share/licenses/poopterm/LICENSE', self.query('--licensefiles').split())
        self.assertIn('/usr/share/doc/poopterm/THIRD_PARTY_NOTICES', self.query('--docfiles').split())
        self.assertEqual(self.query('--queryformat', '%{LICENSE}'), 'AGPL-3.0-or-later')

    def test_the_display_libraries_winit_loads_at_run_time_are_declared(self):
        # winit dlopens them, so rpm's ELF scan cannot see them.
        self.assertIn('libxkbcommon', self.query('--requires').split())
        recommends = self.query('--recommends').split()
        for library in ['libwayland-client', 'libX11', 'libxkbcommon-x11']:
            self.assertIn(library, recommends)

    def test_the_binary_ships_byte_for_byte(self):
        # Releases ship a PGO and BOLT build; stripping or rewriting it would undo that.
        extract = self.temp / 'extract'
        extract.mkdir()
        subprocess.run(f'rpm2cpio {self.package} | cpio -idm --quiet', shell=True, cwd=extract, check=True)
        self.assertEqual((extract / 'usr/bin/poopterm').read_bytes(), self.binary.read_bytes())

    def test_older_packages_in_dist_are_replaced(self):
        stale = ROOT / 'dist/poopterm-0.0.1-1.x86_64.rpm'
        stale.write_bytes(b'stale')
        try:
            subprocess.run(['bash', str(SCRIPT), str(self.binary)], cwd=ROOT, check=True, capture_output=True)
            self.assertFalse(stale.exists())
        finally:
            stale.unlink(missing_ok=True)

    def test_a_missing_binary_fails_with_a_message(self):
        result = subprocess.run(['bash', str(SCRIPT), str(self.temp / 'missing')], cwd=ROOT,
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('executable not found', result.stderr)

    def test_a_binary_without_a_version_fails_with_a_message(self):
        silent = self.temp / 'silent'
        silent.write_text('#!/bin/sh\n')
        silent.chmod(0o755)
        result = subprocess.run(['bash', str(SCRIPT), str(silent)], cwd=ROOT, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('could not read version', result.stderr)


if __name__ == '__main__':
    unittest.main()
