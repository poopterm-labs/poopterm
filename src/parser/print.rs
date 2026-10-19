//! Printable characters and C0/C1 control execution.
use std::sync::Arc;

use crate::{
	graphics::kitty::PLACEHOLDER,
	term::{Cell, NO_EXTRA, Term, attr},
};
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
		let mut template = self.styled_cell();
		if c == PLACEHOLDER {
			// Kitty Unicode placeholders keep the image id as the client set it.
			template.fg = self.fg_id;
		}
		self.print_styled(c, width, template);
	}

	#[inline(always)]
	fn print_styled(&mut self, c: char, width: usize, template: Cell) {
		if self.attach_to_previous(c, width) || width == 0 {
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
		if self.has_multicells {
			// Lower rows of scaled text are skipped; text over a top row
			// erases the whole block.
			self.skip_lower_rows(width);
			self.erase_multicells_in_row(self.cy, self.cx, self.cx + width);
		}
		let (x, y, cols) = (self.cx, self.cy, self.cols);
		let i = self.idx(x, y);
		if self.insert_mode {
			let count = width.min(cols - x);
			self.erase_multicells_for_shift(x, y, count, true);
			self.grid[y].copy_within(x..cols - count, x + count);
			self.clear_range(i, i + count);
			self.repair_wide_rows(y, y + 1);
		}
		self.erase_images(i, i + width);
		let blank = self.blank();
		let row = &mut self.grid[y];
		// Remove both halves when overwriting a wide character.
		if row[x].width == 0 && x > 0 {
			row[x - 1] = blank;
		}
		if row[x].width == 2 && x + 1 < cols {
			row[x + 1] = blank;
		}
		if width == 2 && x + 2 < cols && row[x + 1].width == 2 {
			row[x + 2] = blank;
		}
		let row = &mut self.grid[y];
		row[x] = Cell {
			ch: c,
			width: width as u8,
			..template
		};
		if width == 2 && x + 1 < cols {
			row[x + 1] = Cell {
				width: 0,
				..template
			};
		}
		self.wrap_pending = x + width >= cols;
		self.cx = (x + width).min(cols - 1);
		self.last_printed = Some(c);
	}

	/// Prints text without C0 or C1 controls with one style lookup. Equivalent
	/// to calling [`Term::apply_print`] per character.
	pub(super) fn print_text(&mut self, text: &str) {
		let template = self.styled_cell();
		let blank = self.blank();
		let general = self.charsets[self.active_charset]
			|| self.insert_mode
			|| self.has_multicells
			|| self.has_sixel_images();
		let mut chars = text.chars();
		while let Some(c) = chars.next() {
			debug_assert!(c >= ' ' && !('\u{80}'..='\u{9f}').contains(&c));
			let width = super::utf8::width(c).min(self.cols);
			if general || c == PLACEHOLDER {
				self.apply_print(c);
				continue;
			}
			if width == 0 || self.wrap_pending || self.cx + width > self.cols {
				self.print_styled(c, width, template);
				continue;
			}
			if self.attach_to_previous(c, width) {
				continue;
			}
			// Resolve the row and style once for the whole segment. Zero-width
			// marks and margin handling return to the general path above.
			let cols = self.cols;
			let mut x = self.cx;
			let row = &mut self.grid[self.cy][..cols];
			// Cells inside the segment are all rewritten; only wide characters
			// cut by its edges need repair.
			if row[x].width == 0 && x > 0 {
				row[x - 1] = blank;
			}
			let (mut c, mut width) = (c, width);
			loop {
				row[x] = Cell {
					ch: c,
					width: width as u8,
					..template
				};
				if width == 2 {
					row[x + 1] = Cell {
						width: 0,
						..template
					};
				}
				x += width;
				// Peek: a character that does not fit goes to the general path.
				let before = chars.clone();
				let Some(next) = chars.next() else {
					break;
				};
				let next_width = super::utf8::width(next).min(cols);
				if next_width == 0 || x + next_width > cols || next == PLACEHOLDER {
					chars = before;
					break;
				}
				(c, width) = (next, next_width);
			}
			if x < cols && row[x].width == 0 {
				row[x] = blank;
			}
			self.last_printed = Some(c);
			self.wrap_pending = x >= cols;
			self.cx = x.min(cols - 1);
		}
	}

	/// Prints a run of printable ASCII (`0x20..=0x7e`). Equivalent to calling
	/// [`Term::apply_print`] per byte, but resolves the cell style once and
	/// fills each row segment in one pass.
	pub(crate) fn print_ascii(&mut self, mut bytes: &[u8]) {
		if self.charsets[self.active_charset] || self.insert_mode || self.has_multicells {
			for &byte in bytes {
				self.apply_print(byte as char);
			}
			return;
		}
		// Only the first character can join a cluster (after a ZWJ).
		if let Some((&first, rest)) = bytes.split_first()
			&& self.attach_to_previous(first as char, 1)
		{
			bytes = rest;
		}
		let template = self.styled_cell();
		while !bytes.is_empty() {
			if self.wrap_pending {
				if self.auto_wrap {
					self.cx = 0;
					self.linefeed();
				} else {
					// Without wrap, each character overwrites the last column.
					bytes = &bytes[bytes.len() - 1..];
				}
			}
			self.wrap_pending = false;
			let (x, y, cols) = (self.cx, self.cy, self.cols);
			let count = (cols - x).min(bytes.len());
			if self.has_sixel_images() {
				let i = self.idx(x, y);
				self.erase_images(i, i + count);
			}
			let blank = self.blank();
			let row = &mut self.grid[y];
			// Remove the other half of wide characters cut by the run's edges.
			if row[x].width == 0 && x > 0 {
				row[x - 1] = blank;
			}
			if row[x + count - 1].width == 2 && x + count < cols {
				row[x + count] = blank;
			}
			for (cell, &byte) in row[x..x + count].iter_mut().zip(bytes) {
				*cell = Cell {
					ch: byte as char,
					..template
				};
			}
			self.last_printed = Some(bytes[count - 1] as char);
			bytes = &bytes[count..];
			self.wrap_pending = x + count >= cols;
			self.cx = (x + count).min(cols - 1);
		}
	}

	/// A blank cell carrying the current colors and attributes, with bold
	/// brightening, reverse video, dim and conceal already applied.
	#[inline]
	pub(crate) fn styled_cell(&self) -> Cell {
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
		Cell {
			attrs: self.attrs,
			fg,
			bg,
			extra: self.active_extra,
			..Cell::default()
		}
	}

	/// Appends zero-width marks (and anything after a ZWJ) to the previous
	/// cell's cluster. Returns true when `c` was consumed that way.
	#[inline]
	fn attach_to_previous(&mut self, c: char, width: usize) -> bool {
		let mut x = if self.wrap_pending {
			self.cx
		} else if self.cx > 0 {
			self.cx - 1
		} else {
			return false;
		};
		let row = &self.grid[self.cy];
		if row[x].width == 0 {
			x = x.saturating_sub(1);
		}
		let extra = row[x].extra;
		if self.has_multicells
			&& let Some(part) = self.extras.multicell(extra)
		{
			if width != 0 && !part.text.ends_with('\u{200d}') {
				return false;
			}
			self.extend_multicell(x, self.cy, c);
			return true;
		}
		// Fast path: most cells have no marks, so nothing can join them.
		if width != 0 && (extra == NO_EXTRA || self.extras.marks(extra).last() != Some(&'\u{200d}')) {
			return false;
		}
		if let Some(id) = Arc::make_mut(&mut self.extras).with_mark(extra, c) {
			self.grid[self.cy][x].extra = id;
		}
		true
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
