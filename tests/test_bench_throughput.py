"""End-to-end benchmark payloads: every write ends on a sequence boundary."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('throughput', Path(__file__).parents[1] / 'scripts/bench-throughput.py')
throughput = importlib.util.module_from_spec(spec)
spec.loader.exec_module(throughput)


class Payloads(unittest.TestCase):
    def setUp(self):
        try:
            self.binary = throughput.poopterm_binary()
        except FileNotFoundError:
            self.skipTest('needs target/release/poopterm (cargo build --release)')

    def test_timed_and_warm_up_payloads_end_on_whole_sequences(self):
        # A payload cut inside `CSI 38;2;…` makes the following DA1 query part
        # of that sequence. kitty then drops both, never answers, and the
        # benchmark aborts (closing kitty's window, which looks like a crash).
        for payload in throughput.build_payloads(1, binary=self.binary):
            for data in [payload.data, payload.warm_up]:
                with self.subTest(payload=payload.key, size=len(data)):
                    self.assertIsNone(re.search(rb'\x1b(\[[0-9;:]*)?$', data))
                    self.assertTrue(payload.data.startswith(data))
            self.assertLess(len(payload.warm_up), len(payload.data))


    def test_the_terminal_names_itself_through_xtversion(self):
        # TERM_PROGRAM is inherited from the parent terminal, so kitty started
        # from ghostty would be reported as ghostty.
        reply = b'\x1bP>|kitty(0.49.2)\x1b\\\x1b[?62;c'
        self.assertEqual(throughput.parse_xtversion(reply), 'kitty(0.49.2)')
        self.assertIsNone(throughput.parse_xtversion(b'\x1b[?62;c'))


class BinaryResolution(unittest.TestCase):
    def test_installed_binary_is_used_when_no_local_release_build_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / 'bin/poopterm'
            installed.parent.mkdir()
            installed.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {}, clear=True):
                    with mock.patch.object(shutil, 'which', return_value=str(installed)):
                        self.assertEqual(throughput.poopterm_binary(), installed)

    def test_local_release_binary_takes_precedence_over_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            local = root / 'target/release/poopterm'
            local.parent.mkdir(parents=True)
            local.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {}, clear=True):
                    with mock.patch.object(shutil, 'which', return_value='/usr/bin/poopterm'):
                        self.assertEqual(throughput.poopterm_binary(), local)

    def test_explicit_binary_takes_precedence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            explicit = root / 'custom/poopterm'
            explicit.parent.mkdir()
            explicit.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {'POOPTERM_BIN': str(explicit)}):
                    with mock.patch.object(shutil, 'which', return_value='/usr/bin/poopterm'):
                        self.assertEqual(throughput.poopterm_binary(), explicit)


if __name__ == '__main__':
    unittest.main()
