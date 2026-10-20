"""Profile-guided builds: training runs, the shipped binary, and the fallback."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).parents[1]
HOST = 'x86_64-unknown-test'

# Builds a stand-in poopterm: instrumented builds log their runs and write a
# profile, optimized ones name the profile they used, plain ones say so.
CARGO = '''#!/bin/sh
echo "cargo $*" >> "$FAKE/log"
[ -e "$FAKE/fail-build" ] && exit 1
dir=target/release
while [ $# -gt 0 ]; do
	case $1 in
	--target-dir) dir=$2/%s/release; shift ;;
	esac
	shift
done
mkdir -p "$dir"
case $RUSTFLAGS in
*-Cprofile-generate=*)
	profiles=${RUSTFLAGS##*-Cprofile-generate=}
	cat > "$dir/poopterm" <<EOF
#!/bin/sh
echo "train \\$*" >> "$FAKE/log"
[ -e "$FAKE/fail-train" ] && exit 1
touch "$profiles/run\\$\\$.profraw"
EOF
	;;
*-Cprofile-use=*) printf '#!/bin/sh\\necho optimized with %%s\\n' "${RUSTFLAGS##*-Cprofile-use=}" > "$dir/poopterm" ;;
*) printf '#!/bin/sh\\necho plain\\n' > "$dir/poopterm" ;;
esac
chmod +x "$dir/poopterm"
''' % HOST

RUSTC = f'''#!/bin/sh
case $1 in
-vV) echo "host: {HOST}" ;;
--print) echo "$FAKE/sysroot" ;;
esac
'''

PROFDATA = '''#!/bin/sh
echo "profdata $1 $2 $(($# - 3))" >> "$FAKE/log"
touch "$3"
'''


class Pgo(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.fake = Path(self.temp.name)
        self.repo = self.fake / 'repo'
        (self.repo / 'scripts').mkdir(parents=True)
        shutil.copy(SOURCE / 'scripts/pgo.sh', self.repo / 'scripts/pgo.sh')
        bin_dir = self.fake / 'bin'
        bin_dir.mkdir()
        tools = self.fake / f'sysroot/lib/rustlib/{HOST}/bin'
        tools.mkdir(parents=True)
        for path, text in [(bin_dir / 'cargo', CARGO), (bin_dir / 'rustc', RUSTC),
                           (tools / 'llvm-profdata', PROFDATA)]:
            path.write_text(text)
            path.chmod(0o755)
        self.env = dict(os.environ, FAKE=str(self.fake),
                        PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}")

    def pgo(self, *args):
        return subprocess.run(['sh', 'scripts/pgo.sh', *args], cwd=self.repo, env=self.env,
                              capture_output=True, text=True)

    def log(self):
        return (self.fake / 'log').read_text().splitlines()

    def shipped(self):
        return subprocess.check_output([self.repo / 'target/release/poopterm'], text=True).strip()

    def test_training_covers_floods_and_small_interactive_writes(self):
        result = self.pgo('train', '2')
        self.assertEqual(result.returncode, 0, result.stderr)
        runs = [line for line in self.log() if line.startswith('train')]
        self.assertEqual(runs, ['train --benchmark --mib 2',
                                'train --benchmark --kib 64 --read 16 --frame-every 64'])

    def test_dist_ships_the_optimized_build_trained_on_every_run(self):
        result = self.pgo('dist')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('shipping the PGO build', result.stdout)
        self.assertEqual(self.shipped(), f'optimized with {self.repo}/target/pgo/merged.profdata')
        # Both training runs left a profile, and both were merged.
        self.assertIn('profdata merge -o 2', self.log())
        self.assertNotIn('cargo build --release --locked', self.log())

    def test_dist_passes_the_training_size_on(self):
        self.assertEqual(self.pgo('dist', '3').returncode, 0)
        self.assertIn('train --benchmark --mib 3', self.log())

    def test_failed_training_ships_a_plain_build_and_says_so(self):
        (self.fake / 'fail-train').touch()
        result = self.pgo('dist')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::warning::pgo.sh: PGO failed; shipping a plain release build', result.stderr)
        self.assertEqual(self.shipped(), 'plain')
        self.assertEqual(self.log()[-1], 'cargo build --release --locked')

    def test_a_failed_plain_build_fails_dist(self):
        (self.fake / 'fail-build').touch()
        result = self.pgo('dist')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.repo / 'target/release/poopterm').exists())

    def test_dist_works_from_any_directory(self):
        result = subprocess.run(['sh', './pgo.sh', 'dist'], cwd=self.repo / 'scripts', env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('optimized with', self.shipped())


if __name__ == '__main__':
    unittest.main()
