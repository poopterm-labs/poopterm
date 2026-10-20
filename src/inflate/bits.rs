//! Least-significant-bit-first reader over DEFLATE input.

/// Buffers 56 or more bits at a time. Past the end of the input it supplies
/// zero bytes, and refuses to refill once any of them have been consumed.
/// Copied into locals by hot loops, so that its state stays in registers.
#[derive(Clone, Copy)]
pub(super) struct Bits<'a> {
	data: &'a [u8],
	/// The next input byte to load.
	pos: usize,
	/// Bits above `count` are either zero or the input's next bits, so
	/// loading those bytes again leaves them unchanged.
	buf: u64,
	count: u32,
	/// How many of the top `count` bits are zeros past the end of the input.
	phantom: u32,
}

impl<'a> Bits<'a> {
	pub(super) fn new(data: &'a [u8]) -> Self {
		Self {
			data,
			pos: 0,
			buf: 0,
			count: 0,
			phantom: 0,
		}
	}

	/// Loads input until at least 56 bits are buffered. Returns false once
	/// bits past the end of the input have been consumed.
	#[inline(always)]
	pub(super) fn refill(&mut self) -> bool {
		match self.data[self.pos..].first_chunk::<8>() {
			Some(word) => {
				self.buf |= u64::from_le_bytes(*word) << self.count;
				self.pos += (63 - self.count as usize) / 8;
				self.count |= 56;
				true
			}
			None => self.refill_slow(),
		}
	}

	#[cold]
	fn refill_slow(&mut self) -> bool {
		if self.phantom > self.count {
			return false;
		}
		while self.count <= 56 {
			match self.data.get(self.pos) {
				Some(&byte) => {
					self.buf |= u64::from(byte) << self.count;
					self.pos += 1;
				}
				None => self.phantom += 8,
			}
			self.count += 8;
		}
		true
	}

	/// The buffered bits, next first.
	#[inline(always)]
	pub(super) fn peek(&self) -> u64 {
		self.buf
	}

	/// Drops `n` buffered bits.
	#[inline(always)]
	pub(super) fn consume(&mut self, n: u32) {
		self.buf >>= n;
		self.count -= n;
	}

	/// Takes the next `n` buffered bits (at most 32) as a number.
	#[inline(always)]
	pub(super) fn take(&mut self, n: u32) -> usize {
		let value = self.buf & ((1 << n) - 1);
		self.consume(n);
		value as usize
	}

	/// Skips to the next byte boundary and takes the next `n` input bytes
	/// whole, with nothing buffered afterwards. None if the input ends first.
	pub(super) fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
		if self.phantom > self.count {
			return None;
		}
		// Return the buffered whole bytes to the input, dropping the rest of
		// a partly read one.
		let start = self.pos - ((self.count - self.phantom) / 8) as usize;
		let bytes = self.data.get(start..start.checked_add(n)?)?;
		(self.pos, self.buf, self.count, self.phantom) = (start + n, 0, 0, 0);
		Some(bytes)
	}
}
