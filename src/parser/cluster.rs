//! Grapheme clusters built while printing a run: marks and joined characters
//! appended to the cell just written.
use std::{simd::u32x4, sync::Arc};

use crate::{graphics::kitty::PLACEHOLDER, term::Extras};

/// Joins the marks at the start of `rest`, and after a joiner the characters
/// they join, to the cluster of `cell`, a cell as `Cell::words` lays it out,
/// as printing them one at a time would. Then takes the next character if it
/// fits in `room` columns and is not a placeholder. Returns the bytes of
/// `rest` used and the character taken, with its width.
///
/// `cell` was just written by the run, so it has no marks of its own yet.
#[cold]
#[inline(never)]
pub(super) fn join_marks(
	extras: &mut Arc<Extras>,
	cell: &mut u32x4,
	rest: &str,
	room: usize,
) -> (usize, Option<(char, usize)>) {
	// `extra` is the high half of the last lane.
	let mut extra = (cell[3] >> 16) as u16;
	let (mut chars, mut joining) = (rest.chars(), false);
	let mut used = 0;
	let taken = loop {
		let Some(c) = chars.next() else {
			break None;
		};
		// `room` is less than the terminal's width, so widths need no clamping.
		let width = crate::width::width(c);
		if width != 0 && !joining {
			break (width <= room && c != PLACEHOLDER).then_some((c, width));
		}
		let id = match extras.remembered_mark(extra, c) {
			None => Arc::make_mut(extras).with_mark(extra, c),
			hit => hit,
		};
		extra = id.unwrap_or(extra);
		joining = extras.marks(extra).last() == Some(&'\u{200d}');
		used = rest.len() - chars.as_str().len();
	};
	cell[3] = cell[3] & 0xffff | u32::from(extra) << 16;
	match taken {
		Some((c, _)) => (used + c.len_utf8(), taken),
		None => (used, None),
	}
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
