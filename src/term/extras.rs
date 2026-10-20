//! Rarely used per-cell data (combining marks, OSC 8 hyperlinks, parts of
//! scaled text) kept out of [`Cell`](super::Cell) so cells stay 16 bytes.
use std::{
	collections::{HashMap, hash_map::Entry},
	hash::{Hash, Hasher},
};

use super::multicell::Multicell;

/// The `extra` id of a cell with no marks and no hyperlink.
pub(crate) const NONE: u16 = 0;
/// Combining marks kept per cell; further marks are dropped.
pub(crate) const MAX_MARKS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Extra {
	hyperlink: u16,
	marks: Box<[char]>,
	multicell: Option<Box<Multicell>>,
}

// Entries are hashed for every combining mark printed. SipHash costs
// mostly per write, so the common fields go in one write; SipHash itself
// stays, since programs choose the content.
impl Hash for Extra {
	fn hash<H: Hasher>(&self, state: &mut H) {
		let marks = &self.marks[..self.marks.len().min(MAX_MARKS)];
		let mut bytes = [0u8; 4 + 4 * MAX_MARKS];
		bytes[..2].copy_from_slice(&self.hyperlink.to_ne_bytes());
		bytes[2] = marks.len() as u8;
		bytes[3] = u8::from(self.multicell.is_some());
		for (chunk, &mark) in bytes[4..].as_chunks_mut::<4>().0.iter_mut().zip(marks) {
			*chunk = u32::from(mark).to_ne_bytes();
		}
		state.write(&bytes[..4 + 4 * marks.len()]);
		if let Some(multicell) = &self.multicell {
			multicell.hash(state);
		}
	}
}

/// Slots in the [`Extras::with_mark`] cache.
const MEMO: usize = 256;

/// A cached `with_mark` answer: `id` with `mark` appended is `result`.
#[derive(Clone, Copy)]
struct Memo {
	id: u16,
	result: u16,
	/// `u32::MAX`, never a `char`, in an empty slot.
	mark: u32,
}

fn memo_slot(id: u16, mark: char) -> usize {
	let hash = u32::from(id).wrapping_mul(0x9e37_79b9) ^ u32::from(mark).wrapping_mul(0x85eb_ca6b);
	(hash >> 24) as usize % MEMO
}

/// Interned [`Extra`] entries. Ids are stable for the table's lifetime and
/// identical entries share an id, so the table is bounded by distinct content.
#[derive(Clone)]
pub(crate) struct Extras {
	entries: Vec<Extra>,
	ids: HashMap<Extra, u16>,
	/// Recent `with_mark` answers, so text repeating the same marks skips
	/// building and hashing entries. Ids never change meaning, so answers
	/// stay right; colliding pairs only evict each other.
	memo: [Memo; MEMO],
}

impl Default for Extras {
	fn default() -> Self {
		let none = Extra::default();
		Self {
			ids: HashMap::from([(none.clone(), NONE)]),
			entries: vec![none],
			memo: [Memo {
				id: NONE,
				result: NONE,
				mark: u32::MAX,
			}; MEMO],
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

	/// [`Self::with_mark`]'s answer if it is cached, found without `&mut`, so
	/// callers can skip making a shared table unique.
	#[inline]
	pub(crate) fn remembered_mark(&self, id: u16, mark: char) -> Option<u16> {
		let memo = self.memo[memo_slot(id, mark)];
		((memo.id, memo.mark) == (id, u32::from(mark))).then_some(memo.result)
	}

	/// The id for `id`'s content with `mark` appended, or `None` if the table is full.
	pub(crate) fn with_mark(&mut self, id: u16, mark: char) -> Option<u16> {
		if let Some(result) = self.remembered_mark(id, mark) {
			return Some(result);
		}
		let slot = memo_slot(id, mark);
		let current = self.entries.get(id as usize)?;
		let result = if current.marks.len() >= MAX_MARKS {
			id
		} else {
			let mut marks = Vec::with_capacity(current.marks.len() + 1);
			marks.extend_from_slice(&current.marks);
			marks.push(mark);
			self.intern(Extra {
				hyperlink: current.hyperlink,
				marks: marks.into(),
				multicell: current.multicell.clone(),
			})?
		};
		self.memo[slot] = Memo {
			id,
			result,
			mark: mark.into(),
		};
		Some(result)
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
		match self.ids.entry(extra) {
			Entry::Occupied(entry) => Some(*entry.get()),
			Entry::Vacant(entry) => {
				let id = u16::try_from(self.entries.len()).ok()?;
				self.entries.push(entry.key().clone());
				Some(*entry.insert(id))
			}
		}
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
	fn repeated_marks_resolve_to_the_entries_they_name() {
		let mut extras = Extras::default();
		let mut seen = HashMap::new();
		// More marks per id than the cache has slots, so pairs collide; each
		// is asked for twice.
		for round in 0..2 {
			for base in 0..8u16 {
				let id = extras.with_hyperlink(NONE, base + 1).unwrap();
				for mark in '\u{300}'..char::from_u32(0x300 + 2 * MEMO as u32).unwrap() {
					let marked = extras.with_mark(id, mark).unwrap();
					assert_eq!(extras.marks(marked), [mark]);
					assert_eq!(extras.hyperlink(marked), base + 1);
					assert_eq!(
						*seen.entry((id, mark)).or_insert(marked),
						marked,
						"round {round}"
					);
				}
			}
		}
	}

	#[test]
	fn remembered_marks_are_only_ever_with_marks_answers() {
		let mut extras = Extras::default();
		assert_eq!(extras.remembered_mark(NONE, '\u{301}'), None);
		let accent = extras.with_mark(NONE, '\u{301}').unwrap();
		assert_eq!(extras.remembered_mark(NONE, '\u{301}'), Some(accent));
		assert_eq!(extras.remembered_mark(NONE, '\u{302}'), None);
		assert_eq!(extras.remembered_mark(accent, '\u{301}'), None);
		// Fill every slot several times over: whatever is still cached must
		// be the answer with_mark gives, and asking must not change it.
		for mark in '\u{300}'..char::from_u32(0x300 + 4 * MEMO as u32).unwrap() {
			extras.with_mark(accent, mark).unwrap();
		}
		let mut hits = 0;
		for mark in '\u{300}'..char::from_u32(0x300 + 4 * MEMO as u32).unwrap() {
			if let Some(cached) = extras.remembered_mark(accent, mark) {
				hits += 1;
				assert_eq!(extras.marks(cached), ['\u{301}', mark]);
				assert_eq!(extras.with_mark(accent, mark), Some(cached));
			}
		}
		assert!((1..=MEMO).contains(&hits), "{hits}");
		// Another id whose answer would sit in the same slot is not answered.
		let slot = memo_slot(accent, '\u{300}');
		let other = (0..=u16::MAX).find(|&id| id != accent && memo_slot(id, '\u{300}') == slot);
		extras.with_mark(accent, '\u{300}').unwrap();
		assert_eq!(extras.remembered_mark(other.unwrap(), '\u{300}'), None);
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
