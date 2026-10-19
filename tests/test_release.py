"""Release versioning, codename retention, and coordinated file updates."""
import datetime
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).parents[1] / 'scripts/release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class Release(unittest.TestCase):
    def test_same_month_increments_patch_and_keeps_codename(self):
        self.assertEqual(release.next_version('26.10.7', datetime.date(2026, 10, 18)), '26.10.8')

    def test_new_month_and_year_start_at_zero(self):
        self.assertEqual(release.next_version('26.9.4', datetime.date(2026, 10, 1)), '26.10.0')
        self.assertEqual(release.next_version('26.12.9', datetime.date(2027, 1, 1)), '27.1.0')
        with self.assertRaises(ValueError):
            release.next_version('27.1.0', datetime.date(2026, 12, 31))

    def test_plan_updates_only_package_versions_and_prepends_notes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[package]\nname = "poopterm"\nversion = "26.10.0"\n[package.metadata.poopterm]\ncodename = "Giga Fly"\n[dependencies]\nfoo = "26.10.0"\n')
            (root / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "foo"\nversion = "26.10.0"\n\n[[package]]\nname = "poopterm"\nversion = "26.10.0"\n')
            (root / 'CHANGELOG.md').write_text('# Changelog\n\n## Older\n')
            result = release.prepare(root, datetime.date(2026, 10, 18), ['feat(parser): support OSC 66'])
            self.assertEqual(result, ('26.10.1', 'Giga Fly'))
            self.assertIn('foo = "26.10.0"', (root / 'Cargo.toml').read_text())
            self.assertIn('name = "foo"\nversion = "26.10.0"', (root / 'Cargo.lock').read_text())
            self.assertIn('name = "poopterm"\nversion = "26.10.1"', (root / 'Cargo.lock').read_text())
            self.assertTrue((root / 'CHANGELOG.md').read_text().startswith(
                '# Changelog\n\n## 26.10.1 "Giga Fly" — 2026-10-18\n\n### Added\n\n- Support OSC 66 (parser).\n\n## Older\n'))
            result = release.prepare(root, datetime.date(2026, 11, 1), [])
            self.assertEqual(result[0], '26.11.0')
            self.assertNotEqual(result[1], 'Giga Fly')


    def test_notes_group_conventional_commits_by_type(self):
        commits = [
            'feat(parser): support kitty\'s text sizing protocol\n\nWhy it matters.\n',
            'fix: keep images in the scrollback',
            'perf(term): freeze a frame for synchronized output',
            'docs: describe the new escape sequences',
            'chore(release): 26.10.3 after green CI\n\nPoopterm-Release-For: abc\n',
            'chore(homebrew): point formula at published v26.10.3',
            'build(install)!: install system-wide only\n\nBody.\n\nBREAKING CHANGE: scripts/install.sh needs\nroot.\nCo-Authored-By: Someone <a@b>\n',
            'Merge something without a type',
        ]
        self.assertEqual(release.notes(commits), '''### Breaking changes

- Install system-wide only (install). scripts/install.sh needs root.

### Added

- Support kitty's text sizing protocol (parser).

### Fixed

- Keep images in the scrollback.

### Performance

- Freeze a frame for synchronized output (term).

### Maintenance

- Describe the new escape sequences.
- Merge something without a type.
''')

    def test_notes_without_changes_say_so(self):
        self.assertEqual(release.notes(['chore(release): 26.10.3 after green CI']),
                         '- No changes besides release bookkeeping.\n')


if __name__ == '__main__':
    unittest.main()
