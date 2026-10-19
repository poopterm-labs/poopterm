"""The XDG installer: per-user layout, desktop entry, icons, and uninstall."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1] / 'scripts/install.sh'
SIZES = ['16x16', '32x32', '64x64', '128x128', '256x256', '512x512']


class Install(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.home = self.root / 'home'
        self.home.mkdir()
        self.binary = self.root / 'poopterm'
        self.binary.write_text('#!/bin/sh\n')

    def tearDown(self):
        shutil.rmtree(self.root)

    def run_script(self, *args, **env):
        environment = {'PATH': os.environ['PATH'], 'HOME': str(self.home), 'POOPTERM_BIN': str(self.binary)}
        environment.update(env)
        return subprocess.run(['sh', str(SCRIPT), '--no-build', *args], env=environment,
                              capture_output=True, text=True, check=True)

    def test_user_install_follows_xdg_and_uninstalls_cleanly(self):
        self.run_script()
        local = self.home / '.local'
        binary = local / 'bin/poopterm'
        self.assertTrue(os.access(binary, os.X_OK))
        desktop = (local / 'share/applications/poopterm.desktop').read_text()
        # Desktop sessions often start without ~/.local/bin on PATH.
        self.assertIn(f'\nExec={binary}\n', desktop)
        self.assertIn(f'\nTryExec={binary}\n', desktop)
        self.assertIn('\nStartupWMClass=poopterm\n', desktop)
        for size in SIZES:
            self.assertTrue((local / f'share/icons/hicolor/{size}/apps/poopterm.png').is_file(), size)
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(local / 'share/applications/poopterm.desktop')], check=True)

        self.run_script('--uninstall')
        leftovers = [path for path in local.rglob('*poopterm*')]
        self.assertEqual(leftovers, [])

    def test_xdg_data_home_is_honored(self):
        data = self.root / 'data'
        self.run_script(XDG_DATA_HOME=str(data))
        self.assertTrue((data / 'applications/poopterm.desktop').is_file())
        self.assertTrue((self.home / '.local/bin/poopterm').is_file())

    def test_staged_system_install_uses_the_final_paths(self):
        stage = self.root / 'stage'
        self.run_script(PREFIX='/usr/local', DESTDIR=str(stage))
        desktop = (stage / 'usr/local/share/applications/poopterm.desktop').read_text()
        self.assertIn('\nExec=/usr/local/bin/poopterm\n', desktop)
        self.assertTrue((stage / 'usr/local/bin/poopterm').is_file())
        if shutil.which('tic'):
            self.assertTrue(any((stage / 'usr/local/share/terminfo').rglob('xterm-poopterm')))


if __name__ == '__main__':
    unittest.main()
