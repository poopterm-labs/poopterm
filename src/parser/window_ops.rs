//! XTWINOPS (`CSI Ps ; … t`): size reports and the title stack. Moving,
//! resizing or raising the window is left to the user, and the title is
//! never reported back, since a program could make it a command.
use crate::term::Term;

/// Most titles (and icon names) saved; pushing more drops the oldest.
const MAX_SAVED: usize = 10;

impl Term {
	/// `op` and `which` are the first two parameters, 0 when missing.
	pub(super) fn window_op(&mut self, op: u16, which: u16) {
		let (cols, rows) = (self.cols, self.rows);
		let (cell_width, cell_height) = self.cell_size();
		let reply = match op {
			11 => "\x1b[1t".to_owned(),
			14 => format!("\x1b[4;{};{}t", rows * cell_height, cols * cell_width),
			16 => format!("\x1b[6;{cell_height};{cell_width}t"),
			18 => format!("\x1b[8;{rows};{cols}t"),
			19 => format!("\x1b[9;{rows};{cols}t"),
			22 => return self.push_titles(which),
			23 => return self.pop_titles(which),
			_ => return,
		};
		self.responses.push(reply.into_bytes());
	}

	/// 0 saves the icon name and the title, 1 the icon name, 2 the title.
	fn push_titles(&mut self, which: u16) {
		let push = |stack: &mut Vec<String>, title: &str| {
			if stack.len() == MAX_SAVED {
				stack.remove(0);
			}
			stack.push(title.to_owned());
		};
		if matches!(which, 0 | 1) {
			push(&mut self.icon_stack, &self.icon_name);
		}
		if matches!(which, 0 | 2) {
			push(&mut self.title_stack, &self.title);
		}
	}

	fn pop_titles(&mut self, which: u16) {
		if matches!(which, 0 | 1)
			&& let Some(icon_name) = self.icon_stack.pop()
		{
			self.icon_name = icon_name;
		}
		if matches!(which, 0 | 2)
			&& let Some(title) = self.title_stack.pop()
		{
			self.set_title(title);
		}
	}
}
