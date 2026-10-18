//! Which grid rows changed between two frames, so only those are redrawn.
use crate::term::Frame;

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
/// redrawn (size changed, or Sixel images are shown).
pub(crate) fn grid_damage(prev: &Frame, next: &Frame, allow_scroll: bool) -> Option<GridDamage> {
	if (prev.cols, prev.rows) != (next.cols, next.rows)
		|| !prev.images.is_empty()
		|| !next.images.is_empty()
	{
		return None;
	}
	let rows = next.rows;
	let scroll = next
		.top_line
		.checked_sub(prev.top_line)
		.filter(|&lines| allow_scroll && lines < rows)
		.unwrap_or(0);
	let mut damaged: Vec<bool> = (0..rows)
		.map(|row| {
			let old = row + scroll;
			old >= rows
				|| !next.row_matches(row, prev, old)
				|| next.prompt_mark(row) != prev.prompt_mark(old)
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
	Some(GridDamage {
		scroll,
		rows: damaged,
	})
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
	fn prompt_marks_damage_their_row() {
		let mut screen = Screen::new(3, 2);
		screen.feed("\x1b[?25l");
		let before = screen.frame();
		screen.feed("\x1b]133;A\x07");
		let damage = grid_damage(&before, &screen.frame(), true).unwrap();
		assert_eq!(damaged(&damage), [0]);
	}

	#[test]
	fn a_resize_needs_a_full_redraw() {
		let mut screen = Screen::new(3, 2);
		let before = screen.frame();
		screen.term.resize(4, 2);
		assert_eq!(grid_damage(&before, &screen.frame(), true), None);
	}
}
