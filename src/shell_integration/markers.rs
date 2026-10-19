//! OSC 133 semantic prompt markers.
use std::{
	collections::VecDeque,
	time::{Duration, Instant},
};

/// Commands running at least this long notify when they finish unseen.
pub(crate) const LONG_COMMAND: Duration = Duration::from_secs(10);
/// Markers kept per terminal; older ones are dropped.
const MAX_MARKERS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
	#[default]
	Unknown,
	Prompt,
	Command,
	Output,
	Finished,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
	pub phase: Phase,
	pub row: usize,
	pub col: usize,
	pub exit_code: Option<i32>,
}
/// Markers emitted by shell hooks. Rows are absolute line numbers
/// (`history_base + scrollback.len() + cursor row` when recorded), so they
/// stay valid while lines scroll into the history.
#[derive(Clone, Default)]
pub(crate) struct ShellIntegration {
	pub phase: Phase,
	pub last_exit_code: Option<i32>,
	pub markers: VecDeque<Marker>,
	command_started: Option<Instant>,
	long_command_finished: bool,
}
impl ShellIntegration {
	pub fn record(&mut self, params: &[&[u8]], row: usize, col: usize, now: Instant) {
		let phase = match params.get(1).copied() {
			Some(b"A") => Phase::Prompt,
			Some(b"B") => Phase::Command,
			Some(b"C") => Phase::Output,
			Some(b"D") => Phase::Finished,
			_ => return,
		};
		let exit_code = if phase == Phase::Finished {
			params
				.get(2)
				.and_then(|p| std::str::from_utf8(p).ok())
				.and_then(|s| s.parse().ok())
		} else {
			None
		};
		if phase == Phase::Finished {
			self.last_exit_code = exit_code;
			if let Some(started) = self.command_started.take() {
				self.long_command_finished |= now.duration_since(started) >= LONG_COMMAND;
			}
		}
		if phase == Phase::Output {
			self.last_exit_code = None;
			self.command_started = Some(now);
		}
		self.phase = phase;
		self.markers.push_back(Marker {
			phase,
			row,
			col,
			exit_code,
		});
		if self.markers.len() > MAX_MARKERS {
			self.markers.pop_front();
		}
	}

	fn prompts(&self) -> impl DoubleEndedIterator<Item = &Marker> {
		self.markers.iter().filter(|m| m.phase == Phase::Prompt)
	}

	/// The closest prompt row above `row`.
	pub fn previous_prompt(&self, row: usize) -> Option<usize> {
		self.prompts().rev().map(|m| m.row).find(|&r| r < row)
	}

	/// The closest prompt row below `row`.
	pub fn next_prompt(&self, row: usize) -> Option<usize> {
		self.prompts().map(|m| m.row).find(|&r| r > row)
	}

	/// The most recent finished command's output: its start `(row, col)` and
	/// the row where it ended (exclusive).
	pub fn last_output(&self) -> Option<((usize, usize), usize)> {
		let finished = self
			.markers
			.iter()
			.rposition(|m| m.phase == Phase::Finished)?;
		let output = self
			.markers
			.iter()
			.take(finished)
			.rev()
			.take_while(|m| m.phase != Phase::Finished)
			.find(|m| m.phase == Phase::Output)?;
		Some(((output.row, output.col), self.markers[finished].row))
	}

	/// True once after a command that ran for [`LONG_COMMAND`] or longer finished.
	pub fn take_long_command_finished(&mut self) -> bool {
		std::mem::take(&mut self.long_command_finished)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::{Duration, Instant};

	/// Records `kind` ("A", "B", "C", "D;0", ...) at `row`.
	fn mark(shell: &mut ShellIntegration, kind: &str, row: usize, at: Instant) {
		let mut params: Vec<&[u8]> = vec![b"133"];
		params.extend(kind.split(';').map(str::as_bytes));
		shell.record(&params, row, 0, at);
	}

	/// Two commands: rows 0-3 (exit 0) and rows 4-7 (exit 2), then a prompt at 8.
	fn history(start: Instant) -> ShellIntegration {
		let mut shell = ShellIntegration::default();
		for (base, code) in [(0, "0"), (4, "2")] {
			mark(&mut shell, "A", base, start);
			mark(&mut shell, "B", base, start);
			mark(&mut shell, "C", base + 1, start);
			mark(
				&mut shell,
				&format!("D;{code}"),
				base + 4,
				start + Duration::from_secs(1),
			);
		}
		mark(&mut shell, "A", 8, start);
		shell
	}

	#[test]
	fn prompts_can_be_stepped_through() {
		let shell = history(Instant::now());
		assert_eq!(shell.previous_prompt(8), Some(4));
		assert_eq!(shell.previous_prompt(4), Some(0));
		assert_eq!(shell.previous_prompt(0), None);
		assert_eq!(shell.next_prompt(0), Some(4));
		assert_eq!(shell.next_prompt(5), Some(8));
		assert_eq!(shell.next_prompt(8), None);
	}

	#[test]
	fn the_last_command_output_spans_from_output_start_to_completion() {
		let shell = history(Instant::now());
		assert_eq!(shell.last_output(), Some(((5, 0), 8)));
		assert_eq!(ShellIntegration::default().last_output(), None);
	}

	#[test]
	fn long_commands_are_flagged_once_when_they_finish() {
		let start = Instant::now();
		let mut shell = ShellIntegration::default();
		mark(&mut shell, "C", 1, start);
		mark(&mut shell, "D;0", 2, start + Duration::from_secs(1));
		assert!(!shell.take_long_command_finished());
		mark(&mut shell, "C", 3, start);
		mark(&mut shell, "D;1", 4, start + LONG_COMMAND);
		assert!(shell.take_long_command_finished());
		assert!(!shell.take_long_command_finished());
	}
}
