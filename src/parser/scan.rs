//! SIMD byte classification for the parser's ground-state fast path.
use std::simd::{cmp::SimdPartialOrd, u8x32};

const LANES: usize = 32;

/// Number of leading bytes that are printable ASCII (`0x20..=0x7e`),
/// classifying 32 bytes per step.
#[inline]
pub(crate) fn printable_prefix(bytes: &[u8]) -> usize {
	let (chunks, rest) = bytes.as_chunks::<LANES>();
	for (index, chunk) in chunks.iter().enumerate() {
		let lanes = u8x32::from_array(*chunk);
		let printable = lanes.simd_ge(u8x32::splat(0x20)) & lanes.simd_lt(u8x32::splat(0x7f));
		let mask = printable.to_bitmask();
		if mask != u64::from(u32::MAX) {
			return index * LANES + (!mask).trailing_zeros() as usize;
		}
	}
	let done = chunks.len() * LANES;
	done
		+ rest
			.iter()
			.position(|&byte| !(0x20..0x7f).contains(&byte))
			.unwrap_or(rest.len())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn printable_prefix_stops_at_the_first_non_printable_byte() {
		assert_eq!(printable_prefix(b""), 0);
		assert_eq!(printable_prefix(b"hello\r\n"), 5);
		assert_eq!(printable_prefix(b"~\x7f"), 1);
		assert_eq!(printable_prefix(b"\x1b[m"), 0);
		assert_eq!(printable_prefix("é".as_bytes()), 0);
	}

	#[test]
	fn printable_prefix_handles_every_position_across_vector_lanes() {
		for len in 0..100 {
			for stop in 0..=len {
				let mut bytes = vec![b'a'; len];
				if stop < len {
					bytes[stop] = b'\n';
				}
				assert_eq!(printable_prefix(&bytes), stop, "len {len} stop {stop}");
			}
		}
	}
}
