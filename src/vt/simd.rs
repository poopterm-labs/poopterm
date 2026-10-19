//! SIMD byte classification: how far the parser can go in bulk.
//!
//! Each scan finds the first byte that needs individual attention, testing
//! [`LANES`] bytes per step; shorter tails are scanned one byte at a time.
use std::simd::{Mask, Simd, cmp::SimdPartialEq, cmp::SimdPartialOrd};

/// 32 lanes compile to one AVX2 register or two SSE2/NEON registers.
const LANES: usize = 32;
type Bytes = Simd<u8, LANES>;

#[inline(always)]
fn splat(byte: u8) -> Bytes {
	Bytes::splat(byte)
}

/// Index of the first byte matching `simd`/`scalar` (the same predicate).
#[inline(always)]
fn first(
	bytes: &[u8],
	simd: impl Fn(Bytes) -> Mask<i8, LANES>,
	scalar: impl Fn(u8) -> bool,
) -> usize {
	let (chunks, rest) = bytes.as_chunks::<LANES>();
	for (index, chunk) in chunks.iter().enumerate() {
		let mask = simd(Bytes::from_array(*chunk)).to_bitmask();
		if mask != 0 {
			return index * LANES + mask.trailing_zeros() as usize;
		}
	}
	let done = chunks.len() * LANES;
	done
		+ rest
			.iter()
			.position(|&byte| scalar(byte))
			.unwrap_or(rest.len())
}

/// What a ground-state text run contains, from [`text_run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Text {
	/// Printable ASCII only (`0x20..=0x7e`).
	Ascii,
	/// DEL or UTF-8, but no `0xc2` byte, so no encoded C1 control.
	Wide,
	/// May contain encoded C1 controls (`c2 80..=9f`).
	MaybeC1,
}

/// Ground state: the length of the leading text, up to the first C0 control
/// (ESC included), and what it contains.
#[inline]
pub(crate) fn text_run(bytes: &[u8]) -> (usize, Text) {
	let (chunks, rest) = bytes.as_chunks::<LANES>();
	let (mut wide, mut c2) = (false, false);
	let kind = |wide, c2| match (wide, c2) {
		(false, _) => Text::Ascii,
		(true, false) => Text::Wide,
		(true, true) => Text::MaybeC1,
	};
	for (index, chunk) in chunks.iter().enumerate() {
		let lanes = Bytes::from_array(*chunk);
		let control = lanes.simd_lt(splat(0x20)).to_bitmask();
		let high = lanes.simd_ge(splat(0x7f)).to_bitmask();
		let lead = lanes.simd_eq(splat(0xc2)).to_bitmask();
		if control != 0 {
			let at = control.trailing_zeros();
			let before = (1 << at) - 1;
			wide |= high & before != 0;
			c2 |= lead & before != 0;
			return (index * LANES + at as usize, kind(wide, c2));
		}
		wide |= high != 0;
		c2 |= lead != 0;
	}
	let done = chunks.len() * LANES;
	let at = rest
		.iter()
		.position(|&byte| byte < 0x20)
		.unwrap_or(rest.len());
	wide |= rest[..at].iter().any(|&byte| byte >= 0x7f);
	c2 |= rest[..at].contains(&0xc2);
	(done + at, kind(wide, c2))
}

/// OSC string: the first C0 control or `;` (parameter separator).
#[inline]
pub(crate) fn osc_run(bytes: &[u8]) -> usize {
	first(
		bytes,
		|lanes| lanes.simd_lt(splat(0x20)) | lanes.simd_eq(splat(b';')),
		|byte| byte < 0x20 || byte == b';',
	)
}

/// DCS passthrough: the first CAN, SUB, ESC, DEL or byte >= 0x80.
#[inline]
pub(crate) fn dcs_run(bytes: &[u8]) -> usize {
	first(
		bytes,
		|lanes| ends_string(lanes) | lanes.simd_ge(splat(0x7f)),
		|byte| matches!(byte, 0x18 | 0x1a | 0x1b) || byte >= 0x7f,
	)
}

/// APC, SOS and PM strings: the first CAN, SUB or ESC.
#[inline]
pub(crate) fn string_run(bytes: &[u8]) -> usize {
	first(bytes, ends_string, |byte| {
		matches!(byte, 0x18 | 0x1a | 0x1b)
	})
}

#[inline(always)]
fn ends_string(lanes: Bytes) -> Mask<i8, LANES> {
	lanes.simd_eq(splat(0x18)) | lanes.simd_eq(splat(0x1a)) | lanes.simd_eq(splat(0x1b))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Every scan agrees with its scalar definition at every position, on
	/// both sides of a vector boundary.
	#[test]
	fn scans_match_their_scalar_definitions() {
		let probes = [
			0x00u8, 0x07, 0x18, 0x1a, 0x1b, b';', b'a', 0x7e, 0x7f, 0x80, 0x9c, 0xc3, 0xff,
		];
		for len in [0, 1, 31, 32, 33, 64, 70] {
			for at in 0..len {
				for &probe in &probes {
					let mut bytes = vec![b'x'; len];
					bytes[at] = probe;
					let scalar =
						|predicate: fn(u8) -> bool| bytes.iter().position(|&b| predicate(b)).unwrap_or(len);
					let control = scalar(|b| b < 0x20);
					let text = &bytes[..control];
					let kind = if text.iter().all(|&b| (0x20..0x7f).contains(&b)) {
						Text::Ascii
					} else if text.contains(&0xc2) {
						Text::MaybeC1
					} else {
						Text::Wide
					};
					assert_eq!(text_run(&bytes), (control, kind), "{len} {at} {probe:#x}");
					assert_eq!(osc_run(&bytes), scalar(|b| b < 0x20 || b == b';'));
					assert_eq!(
						dcs_run(&bytes),
						scalar(|b| matches!(b, 0x18 | 0x1a | 0x1b) || b >= 0x7f)
					);
					assert_eq!(
						string_run(&bytes),
						scalar(|b| matches!(b, 0x18 | 0x1a | 0x1b))
					);
				}
			}
		}
	}
}
