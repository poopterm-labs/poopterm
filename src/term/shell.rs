//! Terminal actions built on shell-integration markers (OSC 133).
use super::Term;
use crate::{input::click::cursor_moves, shell_integration::Phase};

impl Term {
	/// Starts a new line unless the cursor is already at the left margin.
	pub(crate) fn fresh_line(&mut self) {
		if self.cx != 0 || self.wrap_pending {
			self.cx = 0;
			self.wrap_pending = false;
			self.linefeed();
		}
	}

	/// The cursor-key presses for a click at screen `row`, `col`, when the
	/// prompt asked for clicks to move the cursor and the click is in the
	/// line being edited. Clicks in the prompt go to the input's start,
	/// clicks past its end just after it.
	pub(crate) fn prompt_click(&self, row: usize, col: usize) -> Option<Vec<u8>> {
		if self.shell.click_events != 0 {
			return self.prompt_click_event(row, col);
		}
		if !self.shell.click_move
			|| self.shell.phase != Phase::Command
			|| row != self.cy
			|| self.scroll_offset != 0
			|| self.primary_screen.is_some()
		{
			return None;
		}
		let cells = &self.grid[row];
		let start = self
			.shell
			.input_start(self.history_base + self.scrollback.len() + row)
			.min(self.cx);
		let end = cells
			.iter()
			.rposition(|cell| cell.width != 0 && cell.ch != ' ')
			.map_or(0, |last| last + 1)
			.max(self.cx);
		let mut target = col.clamp(start, end);
		// The right half of a wide character is the character.
		while target > start && cells.get(target).is_some_and(|cell| cell.width == 0) {
			target -= 1;
		}
		Some(cursor_moves(
			cells,
			self.cx,
			target,
			self.application_cursor,
		))
	}

	/// The left button's press and release at screen `row`, `col` in SGR
	/// encoding, for a shell that asked for click events and is showing
	/// its prompt or reading a command.
	fn prompt_click_event(&self, row: usize, col: usize) -> Option<Vec<u8>> {
		if !matches!(self.shell.phase, Phase::Prompt | Phase::Command)
			|| self.scroll_offset != 0
			|| self.primary_screen.is_some()
		{
			return None;
		}
		let absolute = self.history_base + self.scrollback.len() + row;
		let line = self.shell.prompt_line(absolute)?;
		let y = if self.shell.click_events == 2 {
			absolute - line + 1
		} else {
			row + 1
		};
		let x = col + 1;
		Some(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").into_bytes())
	}

	/// Text of the most recent finished command's output (from its OSC 133 C
	/// marker to its D marker), one line per row with trailing spaces trimmed.
	/// Lines already evicted from the scrollback are omitted.
	pub(crate) fn last_command_output(&self) -> Option<String> {
		let ((start_row, start_col), end_row) = self.shell.last_output()?;
		let first = self.history_base;
		let lines: Vec<String> = (start_row.max(first)..end_row)
			.map_while(|row| {
				let cells = self.row_cells(row - first)?;
				let from = if row == start_row { start_col } else { 0 };
				let mut line = String::new();
				for cell in cells.iter().skip(from).filter(|cell| cell.width != 0) {
					line.push(cell.ch);
					line.extend(self.marks(cell));
				}
				Some(line.trim_end().to_owned())
			})
			.collect();
		Some(lines.join("\n"))
	}

	/// Scrolls the view so the previous (`direction < 0`) or next prompt is at
	/// the top. Returns false when there is no such prompt in reach.
	pub(crate) fn jump_to_prompt(&mut self, direction: isize) -> bool {
		if self.primary_screen.is_some() {
			return false;
		}
		let top = self.history_base + self.scrollback.len() - self.scroll_offset;
		let target = if direction < 0 {
			self.shell.previous_prompt(top)
		} else {
			self.shell.next_prompt(top)
		};
		let Some(index) = target.and_then(|row| row.checked_sub(self.history_base)) else {
			return false;
		};
		self.scroll_offset = self.scrollback.len().saturating_sub(index);
		true
	}
}
