#!/usr/bin/env python3
"""Checks a pull request: a Conventional Commits title (it becomes the squashed
commit on main, and the changelog line) and at least three added lines of
tests for every added line of production code."""
import argparse
from pathlib import Path
import re
import subprocess
import sys

TYPES = 'feat|fix|perf|refactor|docs|test|bench|build|ci|style|chore|revert'
# The summary starts lower case and has no final period.
TITLE = re.compile(rf'({TYPES})(\([a-z0-9._/-]+\))?!?: (?![A-Z\s])(?!.*[.\s]$).+')
RATIO = 3
HUNK = re.compile(r'@@ -\S+ \+(\d+)(?:,(\d+))? @@')
INLINE_TESTS = re.compile(r'\s*(pub(\([a-z]+\))? )?mod \w+ \{')


def title_problem(title):
    if TITLE.fullmatch(title):
        return None
    return f'title {title!r} is not a Conventional Commit: type(scope): summary, ' \
        'with a known type, a lower-case summary and no final period'


def test_lines(path, text):
    """Whether each line of a file is test code, or None if it is not code."""
    lines = text.splitlines()
    name = Path(path).name
    if path.endswith('.py'):
        return [path.startswith('tests/')] * len(lines)
    if not path.endswith('.rs'):
        return None
    if name == 'tests.rs' or name.endswith(('_tests.rs', '_bench.rs')) or '/tests/' in f'/{path}':
        return [True] * len(lines)
    # Inline test modules close the file, as docs/testing.md asks.
    for index, line in enumerate(lines):
        if line.strip() == '#[cfg(test)]':
            following = next((rest for rest in lines[index + 1:] if rest.strip()), '')
            if INLINE_TESTS.match(following):
                return [False] * index + [True] * (len(lines) - index)
    return [False] * len(lines)


def added_lines(diff):
    """The new line numbers added to each file by a `git diff --unified=0`."""
    added, path = {}, None
    for line in diff.splitlines():
        if line.startswith('+++ '):
            path = line[6:] if line.startswith('+++ b/') else None
        elif (hunk := HUNK.match(line)) and path:
            start, length = int(hunk[1]), int(hunk[2] or 1)
            if length:
                added.setdefault(path, []).extend(range(start, start + length))
    return added


def count(added, read):
    """(test lines, code lines) among the added lines, skipping blank lines
    and comments; `read(path)` gives a file's new text."""
    tests = code = 0
    for path, numbers in added.items():
        text = read(path)
        kinds = test_lines(path, text)
        if kinds is None:
            continue
        lines = text.splitlines()
        comment = '#' if path.endswith('.py') else '//'
        for number in numbers:
            if number > len(lines) or not lines[number - 1].strip() or lines[number - 1].strip().startswith(comment):
                continue
            if kinds[number - 1]:
                tests += 1
            else:
                code += 1
    return tests, code


def ratio_problem(tests, code):
    if tests >= RATIO * code:
        return None
    return f'{tests} test lines for {code} lines of code; the rule is {RATIO}:1, ' \
        f'so add {RATIO * code - tests} more lines of tests'


def main(argv, cwd=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True, help='the commit the pull request is based on')
    parser.add_argument('--title', required=True)
    args = parser.parse_args(argv)
    git = lambda *a: subprocess.run(['git', *a], cwd=cwd, check=True, capture_output=True, text=True).stdout
    diff = git('diff', '--unified=0', '--no-color', f'{args.base}...HEAD')
    root = Path(git('rev-parse', '--show-toplevel').strip())
    tests, code = count(added_lines(diff), lambda path: (root / path).read_text(errors='replace'))
    print(f'{tests} test lines, {code} code lines')
    problems = [p for p in [title_problem(args.title), ratio_problem(tests, code)] if p]
    for problem in problems:
        print(f'error: {problem}', file=sys.stderr)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
