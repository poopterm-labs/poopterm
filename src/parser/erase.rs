//! Line erasure, repairing only wide characters cut by the erased boundary.
use crate::term::Term;

impl Term {
	pub(super) fn erase_line(&mut self, mode: u16) {
		let row_start = self.idx(0, self.cy);
		let row_end = row_start + self.cols;
		match mode {
			0 => {
				self.clear_range(self.idx(self.cx, self.cy), row_end);
				if self.cx > 0 && self.grid[self.cy][self.cx - 1].width == 2 {
					let cell = &mut self.grid[self.cy][self.cx - 1];
					cell.ch = ' ';
					cell.width = 1;
				}
			}
			1 => {
				self.clear_range(row_start, self.idx(self.cx + 1, self.cy));
				if self.cx + 1 < self.cols && self.grid[self.cy][self.cx + 1].width == 0 {
					let cell = &mut self.grid[self.cy][self.cx + 1];
					cell.ch = ' ';
					cell.width = 1;
				}
			}
			2 => self.clear_range(row_start, row_end),
			_ => {}
		}
	}
}
