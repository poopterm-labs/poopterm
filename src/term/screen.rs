//! Grid geometry: resize, scrolling, alternate screen and erasing.
use std::sync::Arc;

use super::Term;

impl Term {
	pub(crate) fn resize(&mut self, cols: usize, rows: usize) {
		self.finish_sync();
		let cols = cols.max(1);
		let rows = rows.max(1);
		let blank = self.blank();
		let mut new = vec![blank; cols * rows];
		let (old_cols, old_rows) = (self.cols, self.rows);
		for y in 0..rows.min(old_rows) {
			for x in 0..cols.min(old_cols) {
				new[y * cols + x] = self.cells[y * old_cols + x];
			}
		}
		self.cells = new;
		self.tab_stops.resize_with(cols, || false);
		for (col, val) in self.tab_stops.iter_mut().enumerate().skip(old_cols) {
			*val = col > 0 && col % 8 == 0;
		}
		for line in &mut self.scrollback {
			let line = Arc::make_mut(line);
			line.resize(cols, blank);
			line.truncate(cols);
		}
		self.cols = cols;
		self.rows = rows;
		self.wrap_pending = false;
		self.scroll_top = 0;
		self.scroll_bottom = rows.saturating_sub(1);
		if let Some((primary, cx, cy, ..)) = &mut self.primary_screen {
			let mut resized = vec![blank; cols * rows];
			for y in 0..rows.min(old_rows) {
				for x in 0..cols.min(old_cols) {
					resized[y * cols + x] = primary[y * old_cols + x];
				}
			}
			*primary = resized;
			*cx = (*cx).min(cols - 1);
			*cy = (*cy).min(rows - 1);
		}
		self.cx = self.cx.min(cols - 1);
		self.cy = self.cy.min(rows - 1);
		if let Some(modes) = &mut self.primary_modes {
			modes.3 = false;
		}
		self.repair_wide_cells();
	}

	pub(crate) fn repair_wide_cells(&mut self) {
		for y in 0..self.rows {
			self.repair_wide_row(y);
		}
	}

	pub(crate) fn repair_wide_row(&mut self, y: usize) {
		if y >= self.rows {
			return;
		}
		let row_start = y * self.cols;
		let row = &mut self.cells[row_start..row_start + self.cols];
		for col in 0..row.len() {
			if (row[col].width == 0 && (col == 0 || row[col - 1].width != 2))
				|| (row[col].width == 2 && (col + 1 == row.len() || row[col + 1].width != 0))
			{
				row[col].ch = ' ';
				row[col].width = 1;
			}
		}
	}

	pub(crate) fn scroll_view(&mut self, lines: isize) {
		if self.primary_screen.is_some() {
			return;
		}
		self.scroll_offset = if lines > 0 {
			self
				.scroll_offset
				.saturating_add(lines as usize)
				.min(self.scrollback.len())
		} else {
			self.scroll_offset.saturating_sub(lines.unsigned_abs())
		};
	}

	pub(crate) fn alternate_screen(&mut self, enabled: bool) {
		if enabled && self.primary_screen.is_none() {
			self.primary_images = std::mem::take(&mut self.images);
			self.primary_modes = Some((
				self.scroll_top,
				self.scroll_bottom,
				self.origin_mode,
				self.wrap_pending,
			));
			self.scroll_top = 0;
			self.scroll_bottom = self.rows - 1;
			self.origin_mode = false;
			self.wrap_pending = false;
			let blank = self.blank();
			self.primary_screen = Some((
				std::mem::replace(&mut self.cells, vec![blank; self.cols * self.rows]),
				self.cx,
				self.cy,
				self.fg,
				self.bg,
			));
			self.cx = 0;
			self.cy = 0;
			self.scroll_offset = 0;
		} else if !enabled && let Some((cells, cx, cy, fg, bg)) = self.primary_screen.take() {
			self.images = std::mem::take(&mut self.primary_images);
			self.cells = cells;
			self.cx = cx;
			self.cy = cy;
			self.fg = fg;
			self.bg = bg;
			if let Some((top, bottom, origin, wrap)) = self.primary_modes.take() {
				self.scroll_top = top.min(self.rows - 1);
				self.scroll_bottom = bottom.min(self.rows - 1).max(self.scroll_top);
				self.origin_mode = origin;
				self.wrap_pending = wrap;
			}
			self.scroll_offset = 0;
			self.repair_wide_cells();
		}
	}

	#[inline(always)]
	pub(crate) fn linefeed(&mut self) {
		if self.cy == self.scroll_bottom {
			self.scroll_region_up(1);
		} else if self.cy + 1 < self.rows {
			self.cy += 1;
		}
	}

	pub(crate) fn scroll_region_up(&mut self, count: usize) {
		self.scroll_images(-(count.min(self.scroll_bottom - self.scroll_top + 1) as isize));
		let count = count.min(self.scroll_bottom - self.scroll_top + 1);
		let start = self.scroll_top * self.cols;
		let end = (self.scroll_bottom + 1) * self.cols;
		let history = if self.scroll_top == 0
			&& self.scroll_bottom + 1 == self.rows
			&& self.primary_screen.is_none()
		{
			(0..count)
				.map(|row| {
					let from = start + row * self.cols;
					Arc::new(self.cells[from..from + self.cols].to_vec())
				})
				.collect::<Vec<_>>()
		} else {
			Vec::new()
		};
		self
			.cells
			.copy_within(start + count * self.cols..end, start);
		let blank = self.blank();
		self.cells[end - count * self.cols..end].fill(blank);
		if self.scroll_offset > 0 {
			self.scroll_offset += history.len();
		}
		self.scrollback.extend(history);
		while self.scrollback.len() > self.scrollback_limit {
			self.scrollback.pop_front();
			self.history_base = self.history_base.saturating_add(1);
		}
		self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
	}

	pub(crate) fn scroll_region_down(&mut self, count: usize) {
		self.scroll_images(count.min(self.scroll_bottom - self.scroll_top + 1) as isize);
		let count = count.min(self.scroll_bottom - self.scroll_top + 1);
		let start = self.scroll_top * self.cols;
		let end = (self.scroll_bottom + 1) * self.cols;
		self
			.cells
			.copy_within(start..end - count * self.cols, start + count * self.cols);
		let blank = self.blank();
		self.cells[start..start + count * self.cols].fill(blank);
	}

	#[inline]
	pub(crate) fn clear_range(&mut self, from: usize, to: usize) {
		self.erase_images(from, to);
		let end = to.min(self.cells.len());
		if from < end {
			let blank = self.blank();
			self.cells[from..end].fill(blank);
		}
	}
}
