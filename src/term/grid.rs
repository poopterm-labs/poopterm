//! Row-based screen storage.
//!
//! Each row is its own `Vec<Cell>`, so scrolling rotates row headers instead
//! of moving every cell, and rows entering the screen are blanked in place,
//! in memory that is already in cache.
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
	/// passed to `evict` (oldest first), which returns how many leading cells
	/// to blank; the caller vouches that the rest already equal `blank`.
	pub(crate) fn scroll_up(
		&mut self,
		top: usize,
		bottom: usize,
		count: usize,
		blank: Cell,
		mut evict: impl FnMut(&[Cell]) -> usize,
	) {
		let count = count.min(bottom + 1 - top);
		let region = &mut self.rows[top..=bottom];
		for row in &mut region[..count] {
			let dirty = evict(row);
			Cell::fill(&mut row[..dirty], blank);
		}
		if count == 1 {
			rotate_left_one(region);
		} else {
			region.rotate_left(count);
		}
	}

	/// Scrolls rows `top..=bottom` down by `count`, blanking the rows at the top.
	pub(crate) fn scroll_down(&mut self, top: usize, bottom: usize, count: usize, blank: Cell) {
		let count = count.min(bottom + 1 - top);
		let region = &mut self.rows[top..=bottom];
		region.rotate_right(count);
		for row in &mut region[..count] {
			Cell::fill(row, blank);
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

/// `rows.rotate_left(1)` with one move of the row headers; the slice method
/// copies through a buffer, in three calls, on every line feed.
#[inline]
fn rotate_left_one(rows: &mut [Row]) {
	let Some(last) = rows.len().checked_sub(1) else {
		return;
	};
	let rows = rows.as_mut_ptr();
	// SAFETY: the first row is read out, the others shift down by one inside
	// the slice, and the first is written to the slot that frees up, so every
	// row is owned exactly once again; nothing between can panic.
	unsafe {
		let first = std::ptr::read(rows);
		std::ptr::copy(rows.add(1), rows, last);
		std::ptr::write(rows.add(last), first);
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
	fn scrolling_up_hands_leaving_rows_out_before_blanking_them() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let mut evicted = Vec::new();
		g.scroll_up(0, 2, 2, cell('.'), |row| {
			evicted.push(row.iter().map(|c| c.ch).collect::<String>());
			row.len()
		});
		assert_eq!(evicted, ["aa", "bb"]);
		assert_eq!(text(&g), ["cc", "..", "..", "dd"]);
	}

	#[test]
	fn scrolling_up_by_more_than_the_region_evicts_and_blanks_it_all() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let mut evicted = Vec::new();
		g.scroll_up(1, 2, 9, cell('.'), |row| {
			evicted.push(row.iter().map(|c| c.ch).collect::<String>());
			row.len()
		});
		assert_eq!(evicted, ["bb", "cc"]);
		assert_eq!(text(&g), ["aa", "..", "..", "dd"]);
	}

	#[test]
	fn scrolling_up_by_one_moves_rows_without_copying_them() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let buffers: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		g.scroll_up(0, 3, 1, cell('.'), <[Cell]>::len);
		assert_eq!(text(&g), ["bb", "cc", "dd", ".."]);
		let moved: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		assert_eq!(moved, [buffers[1], buffers[2], buffers[3], buffers[0]]);
	}

	#[test]
	fn scrolling_one_row_regions_up_by_one_blanks_them() {
		for (top, expected) in [
			(0, ["..", "bb", "cc"]),
			(1, ["aa", "..", "cc"]),
			(2, ["aa", "bb", ".."]),
		] {
			let mut g = grid(&["aa", "bb", "cc"]);
			g.scroll_up(top, top, 1, cell('.'), <[Cell]>::len);
			assert_eq!(text(&g), expected, "row {top}");
		}
	}

	#[test]
	fn single_row_scrolls_add_up_to_one_larger_scroll() {
		let lines = ["aa", "bb", "cc", "dd", "ee", "ff"];
		for (top, bottom) in [(0, 5), (1, 4), (2, 3), (0, 2), (3, 5)] {
			for count in 1..=bottom + 1 - top {
				let (mut stepped, mut at_once) = (grid(&lines), grid(&lines));
				let (mut evicted_stepped, mut evicted_at_once) = (Vec::new(), Vec::new());
				for _ in 0..count {
					stepped.scroll_up(top, bottom, 1, cell('.'), |row| {
						evicted_stepped.push(row[0].ch);
						row.len()
					});
				}
				at_once.scroll_up(top, bottom, count, cell('.'), |row| {
					evicted_at_once.push(row[0].ch);
					row.len()
				});
				let case = (top, bottom, count);
				assert_eq!(text(&stepped), text(&at_once), "{case:?}");
				assert_eq!(evicted_stepped, evicted_at_once, "{case:?}");
			}
		}
	}

	#[test]
	fn scrolling_up_by_zero_changes_nothing() {
		let mut g = grid(&["aa", "bb"]);
		g.scroll_up(0, 1, 0, cell('.'), |_| panic!("no row leaves"));
		assert_eq!(text(&g), ["aa", "bb"]);
	}

	#[test]
	fn scrolling_down_by_more_than_the_region_blanks_it_all() {
		let mut g = grid(&["aa", "bb", "cc"]);
		g.scroll_down(0, 1, 5, cell('.'));
		assert_eq!(text(&g), ["..", "..", "cc"]);
	}

	#[test]
	fn blanked_rows_take_every_field_of_the_blank() {
		let mut g = grid(&["ab", "cd"]);
		let blank = Cell {
			bg: 4,
			attrs: 2,
			..cell(' ')
		};
		g.scroll_up(0, 1, 1, blank, <[Cell]>::len);
		assert!(g[1].iter().all(|c| *c == blank));
		assert_eq!(g[0][1].ch, 'd');
	}

	#[test]
	fn only_the_prefix_evict_names_is_blanked() {
		let mut g = grid(&["abc", "def", "ghi"]);
		let mut dirty = [1, 0].into_iter();
		g.scroll_up(0, 2, 2, cell('.'), |_| dirty.next().unwrap());
		assert_eq!(text(&g), ["ghi", ".bc", "def"]);
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
