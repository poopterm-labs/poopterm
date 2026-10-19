//! Unicode placeholders: cells holding U+10EEEE show part of an image. The
//! foreground color holds the image id, and up to three diacritics give the
//! image row, column and the id's most significant byte; missing ones are
//! inferred from the cell to the left.
use super::diacritics::diacritic_value;

pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';

/// What resolution needs from one cell of a row.
pub(crate) struct Cell<'a> {
	pub ch: char,
	/// The foreground color as the client set it: an RGB value, or a
	/// 256-color index.
	pub color: u32,
	pub marks: &'a [char],
}

/// The part of an image a placeholder cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
	pub image: u32,
	pub row: u32,
	pub col: u32,
}

/// Resolves a row of cells from left to right; `None` for cells that are
/// not placeholders.
pub(crate) fn resolve_row<'a>(cells: impl IntoIterator<Item = Cell<'a>>) -> Vec<Option<Slot>> {
	// The previous placeholder: its color, slot and high byte.
	let mut previous: Option<(u32, Slot, u32)> = None;
	cells
		.into_iter()
		.map(|cell| {
			if cell.ch != PLACEHOLDER {
				previous = None;
				return None;
			}
			let mut values = cell.marks.iter().filter_map(|&mark| diacritic_value(mark));
			let (row, col, high) = (values.next(), values.next(), values.next());
			let left = previous.filter(|&(color, ..)| color == cell.color);
			let (row, col, high) = match (row, col, high, left) {
				(Some(row), Some(col), Some(high), _) => (row, col, high),
				(Some(row), Some(col), None, Some((_, slot, high)))
					if slot.row == row && slot.col + 1 == col =>
				{
					(row, col, high)
				}
				(Some(row), Some(col), None, _) => (row, col, 0),
				(Some(row), None, _, Some((_, slot, high))) if slot.row == row => (row, slot.col + 1, high),
				(Some(row), None, ..) => (row, 0, 0),
				(None, .., Some((_, slot, high))) => (slot.row, slot.col + 1, high),
				(None, ..) => (0, 0, 0),
			};
			let slot = Slot {
				image: (high & 0xff) << 24 | cell.color & 0xff_ffff,
				row,
				col,
			};
			previous = Some((cell.color, slot, high));
			Some(slot)
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	const ID: u32 = 0x12_3456;
	/// Diacritics for 0, 1, 2, 3, 4 and 5.
	const D: [char; 6] = [
		'\u{305}', '\u{30d}', '\u{30e}', '\u{310}', '\u{312}', '\u{33d}',
	];

	fn resolve(cells: &[(char, u32, &[char])]) -> Vec<Option<(u32, u32, u32)>> {
		let cells = cells
			.iter()
			.map(|&(ch, color, marks)| Cell { ch, color, marks });
		resolve_row(cells)
			.into_iter()
			.map(|slot| slot.map(|slot| (slot.image, slot.row, slot.col)))
			.collect()
	}

	#[test]
	fn diacritics_give_row_column_and_the_high_byte_of_the_id() {
		let cells: [(char, u32, &[char]); 2] = [
			(PLACEHOLDER, ID, &[D[0], D[0]]),
			(PLACEHOLDER, ID, &[D[1], D[4], D[2]]),
		];
		assert_eq!(
			resolve(&cells),
			[Some((ID, 0, 0)), Some((2 << 24 | ID, 1, 4))]
		);
	}

	#[test]
	fn missing_diacritics_continue_from_the_cell_to_the_left() {
		let cells: [(char, u32, &[char]); 3] = [
			(PLACEHOLDER, ID, &[D[1], D[0], D[2]]),
			(PLACEHOLDER, ID, &[]),
			(PLACEHOLDER, ID, &[D[1]]),
		];
		let high = 2 << 24 | ID;
		assert_eq!(
			resolve(&cells),
			[Some((high, 1, 0)), Some((high, 1, 1)), Some((high, 1, 2))]
		);
		// The high byte carries over only to the next column of the same row.
		let cells: [(char, u32, &[char]); 3] = [
			(PLACEHOLDER, ID, &[D[0], D[0], D[3]]),
			(PLACEHOLDER, ID, &[D[0], D[1]]),
			(PLACEHOLDER, ID, &[D[0], D[5]]),
		];
		assert_eq!(
			resolve(&cells),
			[
				Some((3 << 24 | ID, 0, 0)),
				Some((3 << 24 | ID, 0, 1)),
				Some((ID, 0, 5))
			]
		);
	}

	#[test]
	fn other_colors_rows_and_characters_break_the_inference() {
		let cells: [(char, u32, &[char]); 5] = [
			(PLACEHOLDER, ID, &[D[0], D[3]]),
			(PLACEHOLDER, 7, &[]),
			(PLACEHOLDER, 7, &[D[2]]),
			('x', 7, &[]),
			(PLACEHOLDER, 7, &[]),
		];
		assert_eq!(
			resolve(&cells),
			[
				Some((ID, 0, 3)),
				Some((7, 0, 0)),
				Some((7, 2, 0)),
				None,
				Some((7, 0, 0))
			]
		);
	}

	#[test]
	fn other_marks_are_ignored() {
		let cells: [(char, u32, &[char]); 1] = [(PLACEHOLDER, ID, &['\u{fe0f}', D[1], D[2]])];
		assert_eq!(resolve(&cells), [Some((ID, 1, 2))]);
	}
}
