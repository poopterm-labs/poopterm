//! A tiny non-cryptographic hasher (the "Fx" hash used inside rustc).
//!
//! The glyph cache is looked up once per visible cell per frame, so the
//! default SipHash hasher shows up in profiles. Keys are small integers that
//! no attacker controls, so a cheap multiplicative hash is safe here.
use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Clone, Copy, Default)]
pub(crate) struct FxHasher {
	hash: u64,
}

impl FxHasher {
	#[inline]
	fn add(&mut self, word: u64) {
		self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
	}
}

impl Hasher for FxHasher {
	#[inline]
	fn write(&mut self, bytes: &[u8]) {
		for chunk in bytes.chunks(8) {
			let mut word = [0u8; 8];
			word[..chunk.len()].copy_from_slice(chunk);
			self.add(u64::from_le_bytes(word));
		}
	}
	#[inline]
	fn write_u8(&mut self, value: u8) {
		self.add(value.into());
	}
	#[inline]
	fn write_u32(&mut self, value: u32) {
		self.add(value.into());
	}
	#[inline]
	fn write_u64(&mut self, value: u64) {
		self.add(value);
	}
	#[inline]
	fn write_usize(&mut self, value: usize) {
		self.add(value as u64);
	}
	#[inline]
	fn finish(&self) -> u64 {
		self.hash
	}
}

pub(crate) type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub(crate) type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;

#[cfg(test)]
mod tests {
	use super::*;
	use std::hash::BuildHasher;

	#[test]
	fn distinct_small_keys_hash_differently() {
		let build = FxBuildHasher::default();
		let hashes: std::collections::HashSet<u64> = (0..10_000u32)
			.map(|ch| build.hash_one((ch, 18.0f32.to_bits())))
			.collect();
		assert_eq!(hashes.len(), 10_000);
	}
}
