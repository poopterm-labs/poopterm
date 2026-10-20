//! Bit reader tests, against reading one bit at a time.
use super::bits::Bits;
use crate::bench::corpus::Rng;

/// Bits `from..from + count` of `data`, least significant first, with zeros
/// past its end.
fn reference(data: &[u8], from: usize, count: u32) -> usize {
	(0..count as usize)
		.map(|i| {
			let bit = from + i;
			data
				.get(bit / 8)
				.map_or(0, |byte| (byte >> (bit % 8) & 1) as usize)
				<< i
		})
		.sum()
}

fn random(rng: &mut Rng, size: usize) -> Vec<u8> {
	(0..size).map(|_| rng.below(256) as u8).collect()
}

#[test]
fn reads_match_reading_bit_by_bit() {
	let mut rng = Rng::new(21);
	// Short inputs refill a byte at a time, long ones eight at a time.
	for size in [0, 1, 7, 8, 9, 15, 16, 17, 100, 1000] {
		let data = random(&mut rng, size);
		let mut bits = Bits::new(&data);
		let mut at = 0;
		while at < data.len() * 8 {
			assert!(bits.refill(), "refill at bit {at} of {size} bytes");
			// Up to 56 bits per refill, in reads of up to 24.
			let mut budget = 56;
			while budget > 0 {
				let n = (rng.below(25) as u32).min(budget);
				assert_eq!(bits.take(n), reference(&data, at, n), "{n} bits at {at}");
				(at, budget) = (at + n as usize, budget - n);
			}
		}
	}
}

#[test]
fn peek_shows_the_next_56_bits() {
	let mut rng = Rng::new(22);
	let data = random(&mut rng, 64);
	let mut bits = Bits::new(&data);
	let mut at = 0;
	for _ in 0..40 {
		assert!(bits.refill());
		let mask = (1u64 << 56) - 1;
		assert_eq!(
			bits.peek() & mask,
			reference(&data, at, 56) as u64,
			"at {at}"
		);
		let n = rng.below(16) as u32;
		bits.consume(n);
		at += n as usize;
	}
}

#[test]
fn taking_nothing_changes_nothing() {
	let data = [0b1010_0110, 0xff];
	let mut bits = Bits::new(&data);
	assert!(bits.refill());
	assert_eq!(bits.take(0), 0);
	assert_eq!(bits.take(3), 0b110);
	assert_eq!(bits.take(0), 0);
	assert_eq!(bits.take(5), 0b10100);
	assert_eq!(bits.take(8), 0xff);
}

/// Consumes `count` bits in reads of up to 24, refilling before each, and
/// returns whether every refill succeeded.
fn skip(bits: &mut Bits, count: usize) -> bool {
	let mut left = count;
	let mut refilled = true;
	while left > 0 {
		refilled &= bits.refill();
		let n = left.min(24);
		bits.consume(n as u32);
		left -= n;
	}
	refilled
}

#[test]
fn refills_fail_once_bits_past_the_end_are_consumed() {
	let mut rng = Rng::new(23);
	for size in 0..20 {
		let data = random(&mut rng, size);
		// Consuming every bit is fine; one more is not.
		for consumed in [0, 1, size * 8 / 2, size * 8, size * 8 + 1, size * 8 + 9] {
			let mut bits = Bits::new(&data);
			let within = consumed <= size * 8;
			skip(&mut bits, consumed);
			assert_eq!(bits.refill(), within, "{consumed} bits of {size} bytes");
			// And it stays refused.
			assert_eq!(bits.refill(), within, "{consumed} bits of {size} bytes");
		}
	}
}

#[test]
fn bits_past_the_end_read_as_zeros() {
	let data = [0xff; 3];
	let mut bits = Bits::new(&data);
	assert!(bits.refill());
	assert_eq!(bits.take(20), 0xf_ffff);
	// Four real bits, then zeros.
	assert_eq!(bits.take(12), 0xf);
	assert!(!bits.refill());
}

#[test]
fn bytes_start_at_the_next_byte_boundary() {
	let mut rng = Rng::new(24);
	for size in [6, 12, 40, 300] {
		let data = random(&mut rng, size);
		for consumed in [0, 1, 7, 8, 9, 23, 24, 25] {
			let mut bits = Bits::new(&data);
			assert!(skip(&mut bits, consumed));
			let start = consumed.div_ceil(8);
			let n = rng.below(size - start + 1);
			assert_eq!(
				bits.bytes(n),
				Some(&data[start..start + n]),
				"{n} bytes after {consumed} bits of {size}"
			);
			// Reading carries on after them.
			let at = (start + n) * 8;
			assert!(bits.refill());
			assert_eq!(bits.take(16), reference(&data, at, 16), "after {n} bytes");
		}
	}
}

#[test]
fn bytes_past_the_end_are_refused() {
	let data: Vec<u8> = (0..20).collect();
	for consumed in [0, 5, 8, 16, 100] {
		let mut bits = Bits::new(&data);
		skip(&mut bits, consumed);
		let available = 20usize.saturating_sub(consumed.div_ceil(8));
		assert_eq!(bits.bytes(available + 1), None, "after {consumed} bits");
		let mut bits = Bits::new(&data);
		skip(&mut bits, consumed);
		let expected = (consumed <= 160).then(|| &data[20 - available..]);
		assert_eq!(bits.bytes(available), expected, "after {consumed} bits");
	}
	// A length that would overflow.
	let mut bits = Bits::new(&data);
	assert_eq!(bits.bytes(usize::MAX), None);
}

#[test]
fn no_bytes_still_aligns() {
	let data = [0b1111_0001, 0xab, 0xcd];
	let mut bits = Bits::new(&data);
	assert!(bits.refill());
	assert_eq!(bits.take(1), 1);
	assert_eq!(bits.bytes(0), Some(&[][..]));
	assert!(bits.refill());
	assert_eq!(bits.take(8), 0xab);
	assert_eq!(bits.bytes(1), Some(&[0xcd][..]));
	assert_eq!(bits.bytes(1), None);
}

#[test]
fn a_copy_reads_on_its_own() {
	let data: Vec<u8> = (0..32).collect();
	let mut bits = Bits::new(&data);
	assert!(bits.refill());
	bits.consume(8);
	let mut copy = bits;
	assert_eq!(copy.take(8), 1);
	assert!(copy.refill());
	assert_eq!(copy.take(16), 0x0302);
	// The original is where the copy started.
	assert_eq!(bits.take(8), 1);
	bits = copy;
	assert_eq!(bits.take(8), 4);
}
