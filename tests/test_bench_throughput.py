"""End-to-end benchmark payloads: every write ends on a sequence boundary."""
import importlib.util
from pathlib import Path
import re
import unittest

spec = importlib.util.spec_from_file_location('throughput', Path(__file__).parents[1] / 'scripts/bench-throughput.py')
throughput = importlib.util.module_from_spec(spec)
spec.loader.exec_module(throughput)


class Payloads(unittest.TestCase):
    def test_timed_and_warm_up_payloads_end_on_whole_sequences(self):
        # A payload cut inside `CSI 38;2;…` makes the following DA1 query part
        # of that sequence. kitty then drops both, never answers, and the
        # benchmark aborts (closing kitty's window, which looks like a crash).
        for payload in throughput.build_payloads(1 << 20):
            for data in [payload.data, payload.warm_up]:
                with self.subTest(payload=payload.key, size=len(data)):
                    data.decode('utf-8')
                    self.assertIsNone(re.search(rb'\x1b(\[[0-9;:]*)?$', data))
                    self.assertTrue(payload.data.startswith(data))
            self.assertLess(len(payload.warm_up), len(payload.data))


    def test_the_terminal_names_itself_through_xtversion(self):
        # TERM_PROGRAM is inherited from the parent terminal, so kitty started
        # from ghostty would be reported as ghostty.
        reply = b'\x1bP>|kitty(0.49.2)\x1b\\\x1b[?62;c'
        self.assertEqual(throughput.parse_xtversion(reply), 'kitty(0.49.2)')
        self.assertIsNone(throughput.parse_xtversion(b'\x1b[?62;c'))


if __name__ == '__main__':
    unittest.main()
