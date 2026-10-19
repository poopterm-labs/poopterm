//! Rarely used per-cell data (combining marks, OSC 8 hyperlinks, parts of
//! scaled text) kept out of [`Cell`](super::Cell) so cells stay 16 bytes.
use std::collections::HashMap;

use super::multicell::Multicell;

/// The `extra` id of a cell with no marks and no hyperlink.
pub(crate) const NONE: u16 = 0;
/// Combining marks kept per cell; further marks are dropped.
pub(crate) const MAX_MARKS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Extra {
	hyperlink: u16,
	marks: Box<[char]>,
	multicell: Option<Box<Multicell>>,
}

/// Interned [`Extra`] entries. Ids are stable for the table's lifetime and
/// identical entries share an id, so the table is bounded by distinct content.
#[derive(Clone)]
pub(crate) struct Extras {
	entries: Vec<Extra>,
	ids: HashMap<Extra, u16>,
}

impl Default for Extras {
	fn default() -> Self {
		let none = Extra::default();
		Self {
			ids: HashMap::from([(none.clone(), NONE)]),
			entries: vec![none],
		}
	}
}

impl Extras {
	pub(crate) fn marks(&self, id: u16) -> &[char] {
		self
			.entries
			.get(id as usize)
			.map_or(&[], |extra| &extra.marks)
	}

	/// The OSC 8 hyperlink index for `id` (0 when none).
	pub(crate) fn hyperlink(&self, id: u16) -> u16 {
		self
			.entries
			.get(id as usize)
			.map_or(0, |extra| extra.hyperlink)
	}

	/// The id for `id`'s content with `mark` appended, or `None` if the table is full.
	pub(crate) fn with_mark(&mut self, id: u16, mark: char) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		if current.marks.len() >= MAX_MARKS {
			return Some(id);
		}
		let mut marks = current.marks.to_vec();
		marks.push(mark);
		self.intern(Extra {
			marks: marks.into(),
			..current.clone()
		})
	}

	/// The id for `id`'s marks with hyperlink `link`, or `None` if the table is full.
	pub(crate) fn with_hyperlink(&mut self, id: u16, link: u16) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		if current.hyperlink == link {
			return Some(id);
		}
		self.intern(Extra {
			hyperlink: link,
			..current.clone()
		})
	}

	/// The part of a block of scaled text `id`'s cell holds, if any.
	pub(crate) fn multicell(&self, id: u16) -> Option<&Multicell> {
		self.entries.get(id as usize)?.multicell.as_deref()
	}

	/// The id for `id`'s content holding `multicell`, or `None` if the table is full.
	pub(crate) fn with_multicell(&mut self, id: u16, multicell: Option<Multicell>) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		self.intern(Extra {
			multicell: multicell.map(Box::new),
			..current.clone()
		})
	}

	fn intern(&mut self, extra: Extra) -> Option<u16> {
		if let Some(&id) = self.ids.get(&extra) {
			return Some(id);
		}
		let id = u16::try_from(self.entries.len()).ok()?;
		self.entries.push(extra.clone());
		self.ids.insert(extra, id);
		Some(id)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn plain_cells_need_no_entry() {
		let mut extras = Extras::default();
		assert_eq!(extras.with_hyperlink(NONE, 0), Some(NONE));
		assert!(extras.marks(NONE).is_empty());
		assert_eq!(extras.hyperlink(NONE), 0);
	}

	#[test]
	fn identical_entries_are_shared() {
		let mut extras = Extras::default();
		let a = extras.with_mark(NONE, '\u{301}').unwrap();
		let b = extras.with_mark(NONE, '\u{301}').unwrap();
		assert_eq!(a, b);
		assert_eq!(extras.marks(a), ['\u{301}']);
	}

	#[test]
	fn marks_and_hyperlinks_combine() {
		let mut extras = Extras::default();
		let linked = extras.with_hyperlink(NONE, 7).unwrap();
		let marked = extras.with_mark(linked, '\u{301}').unwrap();
		assert_eq!(extras.hyperlink(marked), 7);
		assert_eq!(extras.marks(marked), ['\u{301}']);
		assert_eq!(extras.hyperlink(linked), 7);
		assert!(extras.marks(linked).is_empty());
	}

	#[test]
	fn mark_count_is_bounded() {
		let mut extras = Extras::default();
		let mut id = NONE;
		for _ in 0..MAX_MARKS + 3 {
			id = extras.with_mark(id, '\u{301}').unwrap();
		}
		assert_eq!(extras.marks(id).len(), MAX_MARKS);
	}

	#[test]
	fn a_full_table_refuses_new_entries() {
		let mut extras = Extras::default();
		for link in 1..=u16::MAX {
			assert!(extras.with_hyperlink(NONE, link).is_some());
		}
		assert_eq!(extras.with_mark(NONE, 'x'), None, "65,536 ids are in use");
		assert!(
			extras.with_hyperlink(NONE, 1).is_some(),
			"existing entries still resolve"
		);
	}
}
