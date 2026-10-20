//! Adler-32 (RFC 1950, section 8.2), summing 32 bytes per step.
use std::simd::{Simd, num::SimdUint, u32x32};

const MODULUS: u64 = 65521;
/// Bytes summed between reductions; the lane sums stay far below 2^32.
const BLOCK: usize = 32 * 128;

pub(super) fn adler32(data: &[u8]) -> u32 {
	// Each byte of a chunk adds to `b` once per byte from it to the chunk's
	// end, and the sum before the chunk once per byte in it.
	let weights = u32x32::from_array(std::array::from_fn(|lane| 32 - lane as u32));
	let (mut a, mut b) = (1u64, 0u64);
	for block in data.chunks(BLOCK) {
		let (chunks, rest) = block.as_chunks::<32>();
		let (mut sums, mut before) = (u32x32::splat(0), u32x32::splat(0));
		for chunk in chunks {
			before += sums;
			sums += Simd::from_array(*chunk).cast::<u32>();
		}
		let summed = 32 * chunks.len() as u64;
		b +=
			summed * a + 32 * u64::from(before.reduce_sum()) + u64::from((sums * weights).reduce_sum());
		a += u64::from(sums.reduce_sum());
		for &byte in rest {
			a += u64::from(byte);
			b += a;
		}
		(a, b) = (a % MODULUS, b % MODULUS);
	}
	(b << 16 | a) as u32
}
