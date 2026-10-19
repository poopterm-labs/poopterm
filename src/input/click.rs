//! Clicks in a shell's input line turned into cursor-key presses, for
//! prompts that ask for it (OSC 133 `cl=`).
use crate::term::Cell;

/// The left or right presses moving the cursor from column `from` to
/// column `to` of `cells`, counting characters: a wide one counts once.
pub(crate) fn cursor_moves(
	cells: &[Cell],
	from: usize,
	to: usize,
	application_cursor: bool,
) -> Vec<u8> {
	let (start, end, letter) = if to >= from {
		(from, to, b'C')
	} else {
		(to, from, b'D')
	};
	let characters = cells
		.get(start..end.min(cells.len()))
		.unwrap_or_default()
		.iter()
		.filter(|cell| cell.width != 0)
		.count();
	let key = if application_cursor {
		[0x1b, b'O', letter]
	} else {
		[0x1b, b'[', letter]
	};
	key.repeat(characters)
}
