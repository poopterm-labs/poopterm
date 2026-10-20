//! Canonical Huffman decoding tables (RFC 1951, section 3.2.2).
//!
//! An entry packs the code length in bits 0..4, the kind in bits 4..7, a
//! count of extra bits in bits 8..12 and the value from bit 16 on.

pub(super) const KIND: u32 = 7 << 4;
/// A literal byte, or a code length symbol.
pub(super) const LITERAL: u32 = 0;
/// A length or distance: the value is the base, plus the extra bits.
pub(super) const BASE: u32 = 1 << 4;
pub(super) const END: u32 = 2 << 4;
/// Points at the subtable for longer codes: the value is its offset, and
/// the extra bits field its size in bits.
const LINK: u32 = 3 << 4;
/// A code that is not assigned, or a symbol that is not allowed.
pub(super) const INVALID: u32 = 4 << 4;

/// A table resolving codes of up to `log2(SIZE)` bits in one lookup; longer
/// codes go through a subtable.
pub(super) struct Table<const SIZE: usize> {
	primary: [u32; SIZE],
	sub: Vec<u32>,
}

impl<const SIZE: usize> Table<SIZE> {
	const PRIMARY: u32 = SIZE.trailing_zeros();

	pub(super) fn new() -> Self {
		Self {
			primary: [INVALID; SIZE],
			sub: Vec::new(),
		}
	}

	/// The entry for the code at the start of `bits`.
	#[inline(always)]
	pub(super) fn entry(&self, bits: u64) -> u32 {
		self.resolve(self.first(bits), bits)
	}

	/// The primary table's entry for the code at the start of `bits`: the
	/// code's own entry, unless it is longer than the primary table covers.
	#[inline(always)]
	pub(super) fn first(&self, bits: u64) -> u32 {
		self.primary[bits as usize & (SIZE - 1)]
	}

	/// The entry for the code at the start of `bits`, given `first(bits)`.
	#[inline(always)]
	pub(super) fn resolve(&self, entry: u32, bits: u64) -> u32 {
		if entry & KIND != LINK {
			return entry;
		}
		let index = (bits >> Self::PRIMARY) as usize & ((1 << extra(entry)) - 1);
		self.sub[(entry >> 16) as usize + index]
	}

	/// How many subtable entries the table holds.
	#[cfg(test)]
	pub(super) fn subtable_len(&self) -> usize {
		self.sub.len()
	}

	/// Builds the table for codes of `lengths[symbol]` bits (0 for unused
	/// symbols), each decoding to `entry(symbol)`. Refuses over-subscribed
	/// codes, and incomplete ones unless they have one code at most.
	pub(super) fn build(&mut self, lengths: &[u8], entry: impl Fn(usize) -> u32) -> Option<()> {
		let mut count = [0u32; 16];
		for &length in lengths {
			count[length as usize] += 1;
		}
		count[0] = 0;
		let mut left = 1i32;
		for &codes in &count[1..] {
			left = 2 * left - codes as i32;
			if left < 0 {
				return None;
			}
		}
		if left > 0 && count.iter().sum::<u32>() > 1 {
			return None;
		}
		// The first code of each length, then each symbol's code with its
		// bits reversed, since codes are packed starting with their top bit.
		let mut next = [0u32; 16];
		for length in 1..16 {
			next[length] = (next[length - 1] + count[length - 1]) << 1;
		}
		let mut codes = [0u16; 320];
		let mut longest = [0u8; SIZE];
		for (symbol, &length) in lengths
			.iter()
			.enumerate()
			.filter(|(_, length)| **length != 0)
		{
			let code = next[length as usize];
			next[length as usize] += 1;
			let reversed = (code.reverse_bits() >> (32 - length)) as usize;
			codes[symbol] = reversed as u16;
			let prefix = &mut longest[reversed & (SIZE - 1)];
			*prefix = (*prefix).max(length);
		}
		self.primary.fill(INVALID);
		self.sub.clear();
		for (prefix, &length) in longest.iter().enumerate() {
			if u32::from(length) > Self::PRIMARY {
				let bits = u32::from(length) - Self::PRIMARY;
				self.primary[prefix] = LINK | (self.sub.len() as u32) << 16 | bits << 8 | Self::PRIMARY;
				self.sub.resize(self.sub.len() + (1 << bits), INVALID);
			}
		}
		for (symbol, &length) in lengths
			.iter()
			.enumerate()
			.filter(|(_, length)| **length != 0)
		{
			let (code, length) = (usize::from(codes[symbol]), u32::from(length));
			let value = entry(symbol) | length;
			if length <= Self::PRIMARY {
				for slot in self.primary[code..].iter_mut().step_by(1 << length) {
					*slot = value;
				}
			} else {
				let link = self.primary[code & (SIZE - 1)];
				let start = (link >> 16) as usize;
				let sub = &mut self.sub[start..start + (1 << extra(link))];
				for slot in sub[code >> Self::PRIMARY..]
					.iter_mut()
					.step_by(1 << (length - Self::PRIMARY))
				{
					*slot = value;
				}
			}
		}
		Some(())
	}
}

/// The length of the code an entry decodes, in bits.
#[inline(always)]
pub(super) fn length(entry: u32) -> u32 {
	entry & 15
}

/// The extra bits that follow the code.
#[inline(always)]
pub(super) fn extra(entry: u32) -> u32 {
	(entry >> 8) & 15
}
