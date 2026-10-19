//! OSC 133 semantic prompt markers, with the options of the
//! semantic-prompts proposal (`aid`, `cl`, `k`, `err`) and kitty's
//! (`cmdline`, `cmdline_url`).
use std::{
	collections::VecDeque,
	time::{Duration, Instant},
};

use super::command_line::{percent_decode, unquote};

/// Commands running at least this long notify when they finish unseen.
pub(crate) const LONG_COMMAND: Duration = Duration::from_secs(10);
/// Markers kept per terminal; older ones are dropped.
const MAX_MARKERS: usize = 256;
/// Most nested open commands tracked.
const MAX_OPEN: usize = 16;
/// Longest application identifier and error text kept.
const MAX_AID: usize = 64;
const MAX_ERROR: usize = 32;

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
	/// The last command's `err=` value: any text means it failed.
	pub last_error: Option<String>,
	/// The running or last command line, as the shell reported it.
	pub command_line: Option<String>,
	/// The prompt asked (`cl=`) for clicks in its input line to move the cursor.
	pub click_move: bool,
	pub markers: VecDeque<Marker>,
	/// Open commands, outermost first: their application and whether their
	/// output started.
	open: Vec<(String, bool)>,
	command_started: Option<Instant>,
	long_command_finished: bool,
}

/// The value of option `name` among `options`.
fn option<'a>(options: &[&'a [u8]], name: &str) -> Option<&'a str> {
	options.iter().rev().find_map(|option| {
		let (key, value) = std::str::from_utf8(option).ok()?.split_once('=')?;
		(key == name).then_some(value)
	})
}

/// The command line of a C marker, if it has one: `None` inside when it
/// cannot be decoded. Quoting may leave `;` in a `cmdline`, so it runs to
/// the end of the sequence.
fn command_line(options: &[&[u8]]) -> Option<Option<String>> {
	if let Some(index) = options
		.iter()
		.position(|option| option.starts_with(b"cmdline="))
	{
		let joined = options[index..].join(&b';');
		let text = std::str::from_utf8(&joined[b"cmdline=".len()..]).ok();
		return Some(text.and_then(unquote));
	}
	option(options, "cmdline_url").map(percent_decode)
}

impl ShellIntegration {
	/// Applies OSC 133 `params` (`133`, the marker letter, options), with
	/// the cursor at absolute `row` and `col`.
	pub fn record(&mut self, params: &[&[u8]], row: usize, col: usize, now: Instant) {
		let Some(&letter) = params.get(1) else {
			return;
		};
		let options = &params[2.min(params.len())..];
		match letter {
			b"A" | b"N" => {
				if matches!(option(options, "k"), Some("s" | "c" | "r")) {
					self.phase = Phase::Prompt;
					return;
				}
				let aid = option(options, "aid")
					.filter(|aid| aid.len() <= MAX_AID)
					.unwrap_or("");
				let matching = (letter == b"N")
					.then(|| self.open.iter().rposition(|(open, _)| open == aid))
					.flatten();
				if let Some(index) = matching {
					// A new command from the same application ends the open
					// one and everything nested in it.
					let started = self.open.drain(index..).any(|(_, started)| started);
					if started {
						self.finish(None, None, row, col, now);
					}
				} else if self.open.last().is_some_and(|(_, started)| !started) {
					// A prompt drawn again replaces one that never ran.
					self.open.pop();
				}
				if self.open.len() >= MAX_OPEN {
					self.open.remove(0);
				}
				self.open.push((aid.to_owned(), false));
				self.click_move = option(options, "cl").is_some();
				self.command_line = None;
				self.mark(Phase::Prompt, row, col, None);
			}
			// Further prompts (continuation lines, right prompts) belong to
			// the command already started.
			b"P" => self.phase = Phase::Prompt,
			b"B" | b"I" => self.mark(Phase::Command, row, col, None),
			b"C" => {
				match self.open.last_mut() {
					Some((_, started)) => *started = true,
					None => self.open.push((String::new(), true)),
				}
				// Without one, keep a line reported earlier (OSC 633 E).
				if let Some(line) = command_line(options) {
					self.command_line = line;
				}
				self.last_exit_code = None;
				self.last_error = None;
				self.command_started = Some(now);
				self.mark(Phase::Output, row, col, None);
			}
			b"D" => {
				let (code, options) = match options.first() {
					Some(code) if !code.contains(&b'=') => (*code, &options[1..]),
					_ => (&b""[..], options),
				};
				let mut code = std::str::from_utf8(code)
					.ok()
					.and_then(|code| code.parse().ok());
				// `err=` overrides the exit code: empty is success.
				let error = option(options, "err").map(|error| {
					code = error.parse().ok().or(code);
					error
						.chars()
						.filter(|c| !c.is_control())
						.take(MAX_ERROR)
						.collect::<String>()
				});
				let error = match error {
					Some(error) if error.is_empty() => {
						code = Some(0);
						None
					}
					error => error,
				};
				let aid = option(options, "aid");
				match aid.and_then(|aid| self.open.iter().rposition(|(open, _)| open == aid)) {
					Some(index) => self.open.truncate(index),
					None => {
						self.open.pop();
					}
				}
				self.finish(code, error, row, col, now);
			}
			_ => {}
		}
	}

	fn finish(
		&mut self,
		code: Option<i32>,
		error: Option<String>,
		row: usize,
		col: usize,
		now: Instant,
	) {
		self.last_exit_code = code;
		self.last_error = error;
		if let Some(started) = self.command_started.take() {
			self.long_command_finished |= now.duration_since(started) >= LONG_COMMAND;
		}
		self.mark(Phase::Finished, row, col, code);
	}

	fn mark(&mut self, phase: Phase, row: usize, col: usize, exit_code: Option<i32>) {
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

	/// Where the input on absolute `row` starts: after the prompt that
	/// ended there, else at the margin.
	pub fn input_start(&self, row: usize) -> usize {
		self
			.markers
			.iter()
			.rev()
			.find(|marker| marker.phase == Phase::Command)
			.filter(|marker| marker.row == row)
			.map_or(0, |marker| marker.col)
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
