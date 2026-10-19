"""PTY ceiling benchmark: real reads, short writes, and consumer completion."""
import importlib.util
from pathlib import Path
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PtyCeiling(unittest.TestCase):
    def test_drains_exact_payload_before_returning(self):
        bench = load('bench-pty')
        for payload in [b'hello\n' * 20000, bytes(range(256)) * 1024]:
            seconds, count = bench.measure(payload)
            self.assertEqual(count, len(payload))
            self.assertGreater(seconds, 0)


if __name__ == '__main__':
    unittest.main()
