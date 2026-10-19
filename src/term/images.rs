//! Image placement, scrolling and eviction (Sixel; kitty images are in
//! `kitty.rs`).
use std::sync::Arc;

use super::Term;

impl Term {
	/// Cell size used to work out how many cells an image covers.
	pub(crate) fn cell_size(&self) -> (usize, usize) {
		(self.cell_width.max(1), self.cell_height.max(1))
	}

	pub(crate) fn scroll_images(&mut self, amount: isize) {
		let top = self.scroll_top as isize;
		let bottom = self.scroll_bottom as isize;
		let cell = self.cell_size();
		self.images.retain_mut(|image| {
			let rows = image.cells(cell).1 as isize;
			let last_row = image.row + rows - 1;
			if image.row > bottom || last_row < top {
				return true;
			}
			if image.row < top || last_row > bottom {
				return false;
			}
			image.row += amount;
			image.row >= top && image.row + rows - 1 <= bottom
		});
	}

	pub(crate) fn erase_images(&mut self, from: usize, to: usize) {
		if self.images.is_empty() || from >= to {
			return;
		}
		let cols = self.cols;
		let cell = self.cell_size();
		// Text erases Sixel images under it; kitty images stay.
		self.images.retain(|image| {
			if image.kitty.is_some() {
				return true;
			}
			let (width, height) = image.cells(cell);
			let first_row = image.row.max(0) as usize;
			let end_row = first_row + height;
			let end_col = (image.col + width).min(cols);
			!(first_row..end_row).any(|row| from < row * cols + end_col && to > row * cols + image.col)
		});
	}

	pub(crate) fn finish_sixel(&mut self) {
		let Some(transfer) = self.sixel_transfer.take() else {
			return;
		};
		let Some(bitmap) = transfer.finish(self.bg, self.fg) else {
			return;
		};
		let rows = bitmap.height.div_ceil(self.cell_size().1);
		let (mut row, col) = if self.sixel_scrolling {
			(self.cy as isize, self.cx)
		} else {
			(0, 0)
		};
		let count = bitmap.pixels.len();
		while !self.images.is_empty()
			&& (self.images.len() >= 16
				|| self
					.images
					.iter()
					.map(|image| image.bitmap.pixels.len())
					.sum::<usize>()
					+ count
					> 4 * 1024 * 1024)
		{
			self.images.remove(0);
		}
		if self.sixel_scrolling {
			for _ in 0..rows {
				if self.cy == self.scroll_bottom {
					row -= 1;
				}
				self.linefeed();
			}
			self.cx = 0;
			self.wrap_pending = false;
		}
		self.images.push(crate::graphics::Placement::natural(
			Arc::new(bitmap),
			col,
			row,
		));
	}
}
