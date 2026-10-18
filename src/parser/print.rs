//! Printable characters and C0/C1 control execution.
use crate::term::{Cell, Term, attr};
use unicode_width::UnicodeWidthChar;

impl Term {
	#[inline(always)]
	pub(super) fn apply_print(&mut self, c: char) {
		let c = if self.charsets[self.active_charset] {
			dec_graphic(c)
		} else {
			c
		};
		let width = c.width().unwrap_or(0).min(self.cols);
		let mut previous = if self.wrap_pending {
			self.idx(self.cx, self.cy)
		} else if self.cx > 0 {
			self.idx(self.cx - 1, self.cy)
		} else {
			usize::MAX
		};
		if previous < self.cells.len() && self.cells[previous].width == 0 {
			previous = previous.saturating_sub(1);
		}
		let joins_previous = previous < self.cells.len()
			&& self.cells[previous].combining_len > 0
			&& self.cells[previous].combining[self.cells[previous].combining_len as usize - 1]
				== '\u{200d}';
		if (width == 0 || joins_previous) && previous < self.cells.len() {
			let cell = &mut self.cells[previous];
			if (width == 0 || joins_previous) && (cell.combining_len as usize) < cell.combining.len() {
				cell.combining[cell.combining_len as usize] = c;
				cell.combining_len += 1;
			}
			return;
		}
		if width == 0 {
			return;
		}
		if self.wrap_pending || self.cx + width > self.cols {
			if self.auto_wrap {
				self.cx = 0;
				self.linefeed();
			} else {
				self.cx = self.cols.saturating_sub(width);
			}
		}
		self.wrap_pending = false;
		let i = self.idx(self.cx, self.cy);
		if self.insert_mode {
			let count = width.min(self.cols - self.cx);
			let row_end = (self.cy + 1) * self.cols;
			self.cells.copy_within(i..row_end - count, i + count);
			self.clear_range(i, i + count);
			self.repair_wide_row(self.cy);
		}
		self.erase_images(i, i + width);
		// Remove both halves when overwriting a wide character.
		if self.cells[i].width == 0 && self.cx > 0 {
			self.cells[i - 1] = self.blank();
		}
		if self.cells[i].width == 2 && self.cx + 1 < self.cols {
			self.cells[i + 1] = self.blank();
		}
		if width == 2
			&& self.cx + 1 < self.cols
			&& self.cells[i + 1].width == 2
			&& self.cx + 2 < self.cols
		{
			self.cells[i + 2] = self.blank();
		}
		let mut fg = self.fg;
		let mut bg = self.bg;
		if self.bold
			&& let Some(index) = (0..8).find(|&index| self.indexed_color(index as u8) == fg)
		{
			fg = self.indexed_color((index + 8) as u8);
		}
		if self.reverse {
			std::mem::swap(&mut fg, &mut bg);
		}
		if self.attrs & attr::DIM != 0 {
			fg = crate::render::blend::blend(bg, fg, 160);
		}
		if self.attrs & attr::CONCEAL != 0 {
			fg = bg;
		}
		self.cells[i] = Cell {
			attrs: self.attrs,
			ch: c,
			combining: ['\0'; 8],
			combining_len: 0,
			fg,
			bg,
			width: width as u8,
			hyperlink: self.active_hyperlink,
		};
		if width == 2 && self.cx + 1 < self.cols {
			self.cells[i + 1] = Cell {
				attrs: self.attrs,
				ch: ' ',
				combining: ['\0'; 8],
				combining_len: 0,
				fg,
				bg,
				width: 0,
				hyperlink: self.active_hyperlink,
			};
		}
		self.wrap_pending = self.cx + width >= self.cols;
		self.cx = (self.cx + width).min(self.cols - 1);
	}

	#[inline]
	pub(super) fn apply_execute(&mut self, byte: u8) {
		if matches!(byte, 8..=13) {
			self.wrap_pending = false;
		}
		match byte {
			0x0e => self.active_charset = 1,
			0x0f => self.active_charset = 0,
			7 => self.bell = true,
			0x84 => self.linefeed(),
			0x85 => {
				self.cx = 0;
				self.linefeed();
			}
			0x88 => self.tab_stops[self.cx] = true,
			0x8d => {
				if self.cy == self.scroll_top {
					self.scroll_region_down(1);
				} else {
					self.cy = self.cy.saturating_sub(1);
				}
			}
			b'\n' | 0x0b | 0x0c => self.linefeed(),
			b'\r' => self.cx = 0,
			0x08 => self.cx = self.cx.saturating_sub(1),
			b'\t' => {
				self.cx = (self.cx + 1..self.cols)
					.find(|&col| self.tab_stops[col])
					.unwrap_or(self.cols - 1)
			}
			_ => {}
		}
	}
}

/// Maps ASCII to the DEC Special Graphics (line drawing) character set.
pub(crate) fn dec_graphic(c: char) -> char {
	match c {
		'`' => '◆',
		'a' => '▒',
		'f' => '°',
		'g' => '±',
		'j' => '┘',
		'k' => '┐',
		'l' => '┌',
		'm' => '└',
		'n' => '┼',
		'o' => '⎺',
		'p' => '⎻',
		'q' => '─',
		'r' => '⎼',
		's' => '⎽',
		't' => '├',
		'u' => '┤',
		'v' => '┴',
		'w' => '┬',
		'x' => '│',
		'y' => '≤',
		'z' => '≥',
		'{' => 'π',
		'|' => '≠',
		'}' => '£',
		'~' => '·',
		_ => c,
	}
}
