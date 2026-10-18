//! Terminal actions built on shell-integration markers (OSC 133).
use super::Term;

impl Term {
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
