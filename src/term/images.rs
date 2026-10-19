//! Image placement, scrolling and eviction (Sixel; kitty images are in
//! `kitty.rs`).
use std::sync::Arc;

use super::Term;

impl Term {
	/// Cell size used to work out how many cells an image covers.
	pub(crate) fn cell_size(&self) -> (usize, usize) {
		(self.cell_width.max(1), self.cell_height.max(1))
	}

	/// Moves images with scrolled text. When the whole primary screen
	/// scrolls up, images move into the scrollback and are dropped once they
	/// leave it; inside a scroll region, only images entirely within it move.
	pub(crate) fn scroll_images(&mut self, amount: isize) {
		if self.images.is_empty() {
			return;
		}
		let top = self.scroll_top as isize;
		let bottom = self.scroll_bottom as isize;
		let screen_rows = self.rows as isize;
		let whole = top == 0 && bottom + 1 == screen_rows;
		let history = if self.primary_screen.is_none() {
			self.scrollback_limit as isize
		} else {
			0
		};
		let cell = self.cell_size();
		self.images.retain_mut(|image| {
			let rows = image.cells(cell).1 as isize;
			let last_row = image.row + rows - 1;
			if whole {
				// Scrolling down leaves history alone.
				if amount > 0 && image.row < 0 {
					return true;
				}
				image.row += amount;
				return image.row + rows > -history && image.row < screen_rows;
			}
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

	/// Text erases Sixel images only, so printing can skip image checks
	/// without them.
	#[inline]
	pub(crate) fn has_sixel_images(&self) -> bool {
		self.images.iter().any(|image| image.kitty.is_none())
	}

	pub(crate) fn erase_images(&mut self, from: usize, to: usize) {
		if !self.has_sixel_images() || from >= to {
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
			// Only the rows on screen; images in the scrollback have none.
			let first_row = image.row.max(0) as usize;
			let end_row = (image.row + height as isize).max(0) as usize;
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
		// Sixel images have no ids to delete them by, so only the newest are
		// kept; kitty placements have their own store and quota.
		let count = bitmap.pixels.len();
		loop {
			let sixels = self.images.iter().filter(|image| image.kitty.is_none());
			let (len, pixels) = sixels.fold((0, 0), |(len, pixels), image| {
				(len + 1, pixels + image.bitmap.pixels.len())
			});
			if len == 0 || (len < 16 && pixels + count <= 4 * 1024 * 1024) {
				break;
			}
			let oldest = self.images.iter().position(|image| image.kitty.is_none());
			self.images.remove(oldest.unwrap());
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
