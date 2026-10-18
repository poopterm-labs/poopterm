//! Row-based screen storage.
//!
//! Each row is its own `Vec<Cell>`, so scrolling rotates row headers instead
//! of moving every cell, and a row that scrolls off the top can be handed to
//! the scrollback without copying.
use std::ops::{Index, IndexMut};

use super::Cell;

pub(crate) type Row = Vec<Cell>;

#[derive(Clone)]
pub(crate) struct Grid {
	rows: Vec<Row>,
	cols: usize,
}

impl Grid {
	pub(crate) fn new(cols: usize, rows: usize, blank: Cell) -> Self {
		Self {
			rows: (0..rows).map(|_| vec![blank; cols]).collect(),
			cols,
		}
	}

	#[cfg(test)]
	pub(crate) fn iter(&self) -> impl Iterator<Item = &Row> {
		self.rows.iter()
	}

	pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Row> {
		self.rows.iter_mut()
	}

	/// Scrolls rows `top..=bottom` up by `count`. Rows leaving the region are
	/// passed to `evict` (oldest first), which returns a buffer to reuse for
	/// the blank row entering at the bottom, if it has one.
	pub(crate) fn scroll_up(
		&mut self,
		top: usize,
		bottom: usize,
		count: usize,
		blank: Cell,
		mut evict: impl FnMut(Row) -> Option<Row>,
	) {
		let count = count.min(bottom + 1 - top);
		let region = &mut self.rows[top..=bottom];
		region.rotate_left(count);
		let cols = self.cols;
		for slot in &mut region[bottom + 1 - top - count..] {
			let mut fresh = evict(std::mem::take(slot)).unwrap_or_default();
			fresh.clear();
			fresh.resize(cols, blank);
			*slot = fresh;
		}
	}

	/// Scrolls rows `top..=bottom` down by `count`, blanking the rows at the top.
	pub(crate) fn scroll_down(&mut self, top: usize, bottom: usize, count: usize, blank: Cell) {
		let count = count.min(bottom + 1 - top);
		let region = &mut self.rows[top..=bottom];
		region.rotate_right(count);
		for row in &mut region[..count] {
			row.fill(blank);
		}
	}

	/// Resizes to `cols` × `rows`, keeping the top-left content.
	pub(crate) fn resize(&mut self, cols: usize, rows: usize, blank: Cell) {
		self.rows.truncate(rows);
		for row in &mut self.rows {
			row.resize(cols, blank);
		}
		self.rows.resize_with(rows, || vec![blank; cols]);
		self.cols = cols;
	}
}

impl Index<usize> for Grid {
	type Output = Row;

	#[inline(always)]
	fn index(&self, row: usize) -> &Row {
		&self.rows[row]
	}
}

impl IndexMut<usize> for Grid {
	#[inline(always)]
	fn index_mut(&mut self, row: usize) -> &mut Row {
		&mut self.rows[row]
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn cell(ch: char) -> Cell {
		Cell {
			ch,
			..Cell::default()
		}
	}

	fn grid(lines: &[&str]) -> Grid {
		let mut grid = Grid::new(lines[0].len(), lines.len(), cell(' '));
		for (y, line) in lines.iter().enumerate() {
			for (x, ch) in line.chars().enumerate() {
				grid[y][x] = cell(ch);
			}
		}
		grid
	}

	fn text(grid: &Grid) -> Vec<String> {
		grid
			.iter()
			.map(|row| row.iter().map(|c| c.ch).collect())
			.collect()
	}

	#[test]
	fn scrolling_up_moves_rows_out_and_reuses_returned_buffers() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let mut evicted = Vec::new();
		g.scroll_up(0, 2, 2, cell('.'), |row| {
			evicted.push(row.iter().map(|c| c.ch).collect::<String>());
			Some(row)
		});
		assert_eq!(evicted, ["aa", "bb"]);
		assert_eq!(text(&g), ["cc", "..", "..", "dd"]);
	}

	#[test]
	fn scrolling_down_inside_a_region_leaves_other_rows() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		g.scroll_down(1, 2, 1, cell('.'));
		assert_eq!(text(&g), ["aa", "..", "bb", "dd"]);
	}

	#[test]
	fn resize_keeps_top_left_content() {
		let mut g = grid(&["abc", "def"]);
		g.resize(2, 3, cell('.'));
		assert_eq!(text(&g), ["ab", "de", ".."]);
		assert_eq!((g.cols, g.iter().count()), (2, 3));
	}
}
