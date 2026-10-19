//! Bounds-checked big-endian reads from font data. Font files are untrusted,
//! so every read returns `None` past the end instead of panicking.

pub(crate) trait Bytes {
	fn u8_at(&self, at: usize) -> Option<u8>;
	fn u16_at(&self, at: usize) -> Option<u16>;
	fn u32_at(&self, at: usize) -> Option<u32>;
	fn slice_at(&self, at: usize, len: usize) -> Option<&[u8]>;

	fn i16_at(&self, at: usize) -> Option<i16> {
		self.u16_at(at).map(|value| value as i16)
	}
}

impl Bytes for [u8] {
	#[inline]
	fn u8_at(&self, at: usize) -> Option<u8> {
		self.get(at).copied()
	}

	#[inline]
	fn u16_at(&self, at: usize) -> Option<u16> {
		let bytes = self.get(at..at.checked_add(2)?)?;
		Some(u16::from_be_bytes([bytes[0], bytes[1]]))
	}

	#[inline]
	fn u32_at(&self, at: usize) -> Option<u32> {
		let bytes = self.get(at..at.checked_add(4)?)?;
		Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
	}

	#[inline]
	fn slice_at(&self, at: usize, len: usize) -> Option<&[u8]> {
		self.get(at..at.checked_add(len)?)
	}
}

/// A 2.14 fixed-point number, as `glyf` component transforms use.
pub(crate) fn f2dot14(value: u16) -> f32 {
	f32::from(value as i16) / 16384.0
}
