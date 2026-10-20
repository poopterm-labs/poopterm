//! Adler-32 tests, against the checksum's definition.
use super::adler::adler32;
use crate::bench::corpus::Rng;

const MODULUS: u32 = 65521;

/// Adler-32 straight from RFC 1950, one byte at a time.
fn scalar(data: &[u8]) -> u32 {
	let (mut a, mut b) = (1u32, 0u32);
	for &byte in data {
		a = (a + u32::from(byte)) % MODULUS;
		b = (b + a) % MODULUS;
	}
	b << 16 | a
}

fn random(rng: &mut Rng, size: usize) -> Vec<u8> {
	(0..size).map(|_| rng.below(256) as u8).collect()
}

#[test]
fn known_checksums() {
	assert_eq!(adler32(b""), 1);
	assert_eq!(adler32(b"a"), 0x0062_0062);
	assert_eq!(adler32(b"abc"), 0x024d_0127);
	assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
	// The checksum zlib streams carry: miniz's output ends with it.
	let data = b"the quick brown fox jumps over the lazy dog";
	let stream = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
	assert_eq!(stream[stream.len() - 4..], adler32(data).to_be_bytes());
}

#[test]
fn sizes_around_chunks_and_blocks_match_the_definition() {
	let mut rng = Rng::new(3);
	// Around a 32-byte chunk, a 4096-byte block, and zlib's 5552-byte run.
	for size in [
		1, 31, 32, 33, 63, 64, 65, 4095, 4096, 4097, 5552, 8192, 8193, 70_001,
	] {
		let data = random(&mut rng, size);
		assert_eq!(adler32(&data), scalar(&data), "{size} random bytes");
	}
}

#[test]
fn the_largest_bytes_do_not_overflow() {
	// 0xff is the closest every sum comes to overflowing: within a block,
	// in the lane sums, and in the weighted sum across a long input.
	for size in [32, 4096, 4097, 100_000, 1 << 20] {
		let data = vec![0xff; size];
		assert_eq!(adler32(&data), scalar(&data), "{size} bytes of 0xff");
	}
	let data = vec![0xfe; 3 << 20];
	assert_eq!(adler32(&data), scalar(&data));
}

#[test]
fn the_sums_stay_below_the_modulus() {
	let mut rng = Rng::new(9);
	for _ in 0..200 {
		let size = rng.below(10_000);
		let data = random(&mut rng, size);
		let checksum = adler32(&data);
		assert!(checksum & 0xffff < MODULUS, "{checksum:08x}");
		assert!(checksum >> 16 < MODULUS, "{checksum:08x}");
		assert_eq!(checksum, scalar(&data));
	}
}

#[test]
fn single_bytes_weigh_by_their_distance_from_the_end() {
	// One nonzero byte among zeros adds itself to `a`, and to `b` once for
	// every byte from it to the end: a wrong lane weight shows at once.
	for size in [32, 64, 4096, 4128] {
		for at in [0, 1, 15, 16, 31, size / 2 + 1, size - 32, size - 1] {
			let mut data = vec![0; size];
			data[at] = 1;
			let b = (size as u32 + (size - at) as u32) % MODULUS;
			assert_eq!(adler32(&data), b << 16 | 2, "byte {at} of {size}");
		}
	}
}

#[test]
fn checksums_depend_on_order() {
	// Unlike a plain sum, swapping bytes changes `b`.
	assert_ne!(adler32(b"ab"), adler32(b"ba"));
	let mut rng = Rng::new(4);
	let mut data = random(&mut rng, 5000);
	(data[100], data[4900]) = (1, 2);
	let mut swapped = data.clone();
	swapped.swap(100, 4900);
	assert_ne!(adler32(&data), adler32(&swapped));
	assert_eq!(adler32(&swapped), scalar(&swapped));
}
