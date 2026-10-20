"""Pin who can start the Claude workflows and what each run may spend or touch."""
from pathlib import Path
import re
import unittest

WORKFLOWS = Path(__file__).parents[1] / '.github/workflows'
MENTION = (WORKFLOWS / 'claude.yml').read_text()
REVIEW = (WORKFLOWS / 'claude-code-review.yml').read_text()
MAINTAINERS = """contains(fromJSON('["OWNER", "MEMBER"]'), github.event."""


def condition(workflow):
    """The job's `if:` block, up to the next job key."""
    block = workflow.split('    if: |\n', 1)[1]
    return block.split('\n    runs-on:', 1)[0]


class Mentions(unittest.TestCase):
    def test_every_trigger_requires_a_maintainer(self):
        # Anyone can comment on a public repository; each run spends our usage.
        gate = condition(MENTION)
        # Organization members only: outside collaborators cannot start runs.
        self.assertNotIn('COLLABORATOR', MENTION)
        for event, author in [('issue_comment', 'comment'), ('pull_request_review_comment', 'comment'),
                              ('pull_request_review', 'review'), ('issues', 'issue')]:
            clause = re.search(rf"\(github\.event_name == '{event}' .*?\)\)(?: \|\||\n|$)", gate, re.S)
            self.assertIsNotNone(clause, event)
            self.assertIn(f'{MAINTAINERS}{author}.author_association)', clause[0], event)
        self.assertEqual(gate.count(MAINTAINERS), 4)

    def test_the_claude_label_starts_a_run_on_an_issue(self):
        # Labelling needs triage access, and the action checks the labeller can write.
        self.assertIn('  issues:\n    types: [opened, assigned, labeled]\n', MENTION)
        self.assertIn("(github.event.action == 'labeled' && github.event.label.name == 'claude')", condition(MENTION))
        self.assertIn('label_trigger: claude', MENTION)

    def test_line_comments_on_code_can_start_a_run(self):
        self.assertIn('  pull_request_review_comment:\n    types: [created]\n', MENTION)

    def test_runs_have_the_toolchain_the_checks_need(self):
        self.assertIn('libxkbcommon-dev libwayland-dev', MENTION)
        self.assertIn('rustup toolchain install nightly --profile minimal --component rustfmt --component clippy',
                      MENTION)
        # pr_check.py diffs against origin/main, which a shallow clone lacks.
        self.assertIn('fetch-depth: 0', MENTION)

    def test_claude_may_run_the_checks_but_not_push_or_merge_by_shell(self):
        tools = MENTION.split('--allowed-tools', 1)[1]
        for allowed in ['Bash(cargo test *)', 'Bash(cargo clippy *)', 'Bash(cargo fmt *)',
                        'Bash(python3 scripts/pr_check.py *)', 'Bash(gh pr checks *)']:
            self.assertIn(f'"{allowed}"', tools)
        for forbidden in ['git push', 'gh pr merge', 'Bash(git *)', 'Bash(gh *)', 'Bash(*)', ' Bash ']:
            self.assertNotIn(forbidden, tools)

    def test_mentions_can_ask_for_the_project_review(self):
        self.assertIn("plugins: 'code-review@claude-code-plugins'", MENTION)

    def test_one_run_at_a_time_per_issue_or_pull_request(self):
        self.assertIn('group: claude-${{ github.event.issue.number || github.event.pull_request.number }}', MENTION)
        self.assertIn('cancel-in-progress: false', MENTION)


class Review(unittest.TestCase):
    def test_reviews_run_once_per_pull_request_not_per_push(self):
        types = re.search(r'pull_request:\n    types: \[(.*)\]', REVIEW)[1].split(', ')
        self.assertEqual(sorted(types), ['opened', 'ready_for_review', 'reopened'])
        self.assertNotIn('synchronize', REVIEW)

    def test_drafts_forks_and_dependabot_are_skipped(self):
        gate = condition(REVIEW)
        self.assertIn('!github.event.pull_request.draft', gate)
        self.assertIn('github.event.pull_request.head.repo.full_name == github.repository', gate)
        self.assertIn("github.event.pull_request.user.login != 'dependabot[bot]'", gate)

    def test_the_review_may_only_comment(self):
        self.assertIn('--allowed-tools "mcp__github_inline_comment__create_inline_comment"\'', REVIEW)
        self.assertNotIn('contents: write', REVIEW)

    def test_review_findings_link_to_a_fix(self):
        self.assertIn('include_fix_links: true', REVIEW)

    def test_the_trigger_is_pull_request_never_pull_request_target(self):
        # pull_request_target would run with secrets on a fork's code.
        for workflow in [MENTION, REVIEW]:
            self.assertNotIn('pull_request_target', workflow)


class Both(unittest.TestCase):
    def test_both_run_opus(self):
        for workflow in [MENTION, REVIEW]:
            self.assertEqual(re.findall(r'--model (\S+)', workflow), ['claude-opus-5-5'])

    def test_the_action_is_pinned_to_a_commit(self):
        # Like every third-party action here; Dependabot moves the pin.
        for workflow in [MENTION, REVIEW]:
            uses = re.findall(r'uses: anthropics/claude-code-action@(\S+)', workflow)
            self.assertEqual(len(uses), 1)
            self.assertRegex(uses[0], r'^[0-9a-f]{40}$')

    def test_checkout_matches_the_rest_of_ci(self):
        for workflow in [MENTION, REVIEW]:
            self.assertIn('uses: actions/checkout@v7', workflow)
            self.assertNotIn('@v4', workflow)

    def test_the_workflow_token_stays_read_only(self):
        # Writes go through the Claude app's token, never GITHUB_TOKEN.
        for workflow in [MENTION, REVIEW]:
            self.assertIn('permissions:\n  contents: read\n', workflow)
            self.assertNotRegex(workflow, r'(contents|pull-requests|issues): write')

    def test_runs_have_a_time_limit(self):
        for workflow in [MENTION, REVIEW]:
            self.assertRegex(workflow, r'timeout-minutes: \d+')

    def test_the_condition_lookup_needs_a_condition(self):
        with self.assertRaises(IndexError):
            condition('jobs:\n  x:\n    runs-on: ubuntu-latest\n')


if __name__ == '__main__':
    unittest.main()
