//! Which grid rows changed between two frames, so only those are redrawn.
use std::sync::Arc;

use crate::{graphics::Placement, term::Frame};

/// The work needed to turn the previous frame's pixels into the next one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GridDamage {
	/// Shift the old pixels up by this many rows before drawing.
	pub scroll: usize,
	/// Rows to redraw, after scrolling.
	pub rows: Vec<bool>,
}

#[cfg(test)]
impl GridDamage {
	pub(crate) fn is_empty(&self) -> bool {
		self.scroll == 0 && !self.rows.contains(&true)
	}
}

/// Compares `next` with `prev`. With `allow_scroll`, lines that moved up
/// (output scrolled into history) are matched against their old position
/// and shifted instead of redrawn. Returns `None` when everything must be
/// redrawn (size changed, or text scrolled under visible images).
pub(crate) fn grid_damage(prev: &Frame, next: &Frame, allow_scroll: bool) -> Option<GridDamage> {
	if (prev.cols, prev.rows, prev.cell) != (next.cols, next.rows, next.cell) {
		return None;
	}
	let rows = next.rows;
	let scroll = next
		.top_line
		.checked_sub(prev.top_line)
		.filter(|&lines| allow_scroll && lines < rows)
		.unwrap_or(0);
	let images = !prev.images.is_empty() || !next.images.is_empty();
	if scroll > 0 && images {
		return None;
	}
	let mut damaged: Vec<bool> = (0..rows)
		.map(|row| {
			let old = row + scroll;
			old >= rows || !next.row_matches(row, prev, old)
		})
		.collect();
	// The cursor is drawn over its cell: erase it where it was (after the
	// shift) and draw it where it is.
	if scroll > 0 || prev.cursor != next.cursor || prev.cursor_shape != next.cursor_shape {
		if let Some(row) = prev.cursor.and_then(|(_, row)| row.checked_sub(scroll)) {
			damaged[row] = true;
		}
		if let Some((_, row)) = next.cursor {
			damaged[row] = true;
		}
	}
	if images {
		damage_images(prev, next, &mut damaged);
		damage_images(next, prev, &mut damaged);
	}
	Some(GridDamage {
		scroll,
		rows: damaged,
	})
}

/// Marks the rows covered by images of `frame` that `other` does not show
/// in the same way at the same place.
fn damage_images(frame: &Frame, other: &Frame, damaged: &mut [bool]) {
	let screen_row = |frame: &Frame, image: &Placement| image.row + frame.scroll_offset as isize;
	for image in &frame.images {
		let unchanged = other.images.iter().any(|shown| {
			Arc::ptr_eq(&image.bitmap, &shown.bitmap)
				&& screen_row(frame, image) == screen_row(other, shown)
				&& (image.col, image.offset, image.source, image.extent, image.z)
					== (shown.col, shown.offset, shown.source, shown.extent, shown.z)
		});
		if unchanged {
			continue;
		}
		let top = screen_row(frame, image);
		let bottom = top + image.cells(frame.cell).1 as isize;
		for row in top.max(0)..bottom.min(damaged.len() as isize) {
			damaged[row as usize] = true;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};

	struct Screen {
		term: Term,
		parser: Parser,
	}

	impl Screen {
		fn new(cols: usize, rows: usize) -> Self {
			Self {
				term: Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into()),
				parser: Parser::new(),
			}
		}

		fn feed(&mut self, input: &str) -> &mut Self {
			self.parser.advance(&mut self.term, input.as_bytes());
			self
		}

		fn frame(&self) -> Frame {
			let mut frame = Frame::default();
			self.term.snapshot(&mut frame);
			frame
		}
	}

	fn damaged(damage: &GridDamage) -> Vec<usize> {
		(0..damage.rows.len()).filter(|&r| damage.rows[r]).collect()
	}

	#[test]
	fn identical_frames_need_nothing() {
		let mut screen = Screen::new(4, 3);
		screen.feed("ab");
		let damage = grid_damage(&screen.frame(), &screen.frame(), true).unwrap();
		assert!(damage.is_empty());
	}

	#[test]
	fn a_changed_cell_damages_its_row_and_the_cursor_rows() {
		let mut screen = Screen::new(4, 3);
		screen.feed("\x1b[?25l");
		let before = screen.frame();
		screen.feed("\x1b[2;1Hx");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [1]);
		screen.feed("\x1b[?25h");
		let before = screen.frame();
		screen.feed("\x1b[3;3H");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [1, 2], "old and new cursor rows");
	}

	#[test]
	fn scrolled_lines_are_shifted_not_redrawn() {
		let mut screen = Screen::new(3, 4);
		screen.feed("\x1b[?25l1\r\n2\r\n3\r\n4");
		let before = screen.frame();
		screen.feed("\r\n5\r\n6");
		let after = screen.frame();
		let damage = grid_damage(&before, &after, true).unwrap();
		assert_eq!(damage.scroll, 2);
		assert_eq!(damaged(&damage), [2, 3]);
		let damage = grid_damage(&before, &after, false).unwrap();
		assert_eq!(damage.scroll, 0);
		assert_eq!(damaged(&damage), [0, 1, 2, 3]);
	}

	#[test]
	fn a_scroll_beyond_the_screen_redraws_everything() {
		let mut screen = Screen::new(3, 2);
		screen.feed("\x1b[?25la");
		let before = screen.frame();
		screen.feed("\r\nb\r\nc\r\nd");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damage.scroll, 0);
		assert_eq!(damaged(&damage), [0, 1]);
	}

	#[test]
	fn changed_marks_damage_a_row_even_with_the_same_local_ids() {
		let mut screen = Screen::new(3, 2);
		screen.feed("\x1b[?25le\u{301}");
		let before = screen.frame();
		screen.feed("\re\u{300}\x1b[K");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [0]);
	}

	#[test]
	fn prompt_markers_draw_nothing() {
		// Exit status is the shell's to show; markers only drive prompt jumps.
		let mut screen = Screen::new(3, 2);
		screen.feed("\x1b[?25l");
		let before = screen.frame();
		screen.feed("\x1b]133;A\x07");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [0usize; 0]);
	}

	/// A kitty command placing an opaque `width`×`height` image at the cursor
	/// without moving it.
	fn image(control: &str, width: usize, height: usize) -> String {
		let pixels = vec![200; width * height * 3];
		let payload = String::from_utf8(crate::bench::images::base64(&pixels)).unwrap();
		format!("\x1b_Ga=T,f=24,s={width},v={height},C=1,q=2,{control};{payload}\x1b\\")
	}

	#[test]
	fn unchanged_images_allow_partial_redraws() {
		let mut screen = Screen::new(4, 4);
		screen.feed("\x1b[?25l").feed(&image("i=1", 10, 30));
		let before = screen.frame();
		screen.feed("\x1b[4;1Hx");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [3]);
	}

	#[test]
	fn placed_moved_or_deleted_images_damage_the_rows_they_cover() {
		let mut screen = Screen::new(4, 5);
		screen.feed("\x1b[?25l");
		let before = screen.frame();
		screen.feed("\x1b[2;1H").feed(&image("i=1,p=1", 10, 30));
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [1, 2]);
		let before = screen.frame();
		screen.feed("\x1b[4;2H\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [1, 2, 3, 4]);
		let before = screen.frame();
		screen.feed("\x1b_Ga=d\x1b\\");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [3, 4]);
	}

	#[test]
	fn images_in_history_are_not_in_the_frame_but_visible_ones_stop_scrolling() {
		let mut screen = Screen::new(4, 3);
		screen.feed("\x1b[?25l").feed(&image("i=1", 10, 20));
		let before = screen.frame();
		screen.feed("\x1b[3;1H\n");
		assert_eq!(grid_damage(&before, &screen.frame(), true), None);
		screen.feed("\n\n");
		assert!(screen.frame().images.is_empty());
		let before = screen.frame();
		screen.feed("\n");
		assert_eq!(
			grid_damage(&before, &screen.frame(), true).unwrap().scroll,
			1
		);
	}

	#[test]
	fn a_resize_needs_a_full_redraw() {
		let mut screen = Screen::new(3, 2);
		let before = screen.frame();
		screen.term.resize(4, 2);
		assert_eq!(grid_damage(&before, &screen.frame(), true), None);
	}
}
