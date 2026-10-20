//! Whole-stream tests: match copies against copying byte by byte, how the
//! output grows, random sequences of blocks in random codes, and the
//! checksum.
use super::{
	DISTANCES, FAST_ROOM, Inflater, LENGTHS,
	blocks_tests::stored,
	copy,
	huffman_tests::{canonical, complete},
	tests::{LIMIT, Writer, agrees, compress, dynamic_block, sample},
	zlib,
};
use crate::bench::corpus::Rng;

fn random(rng: &mut Rng, size: usize) -> Vec<u8> {
	(0..size).map(|_| rng.below(256) as u8).collect()
}

#[test]
fn copies_match_copying_byte_by_byte() {
	let mut rng = Rng::new(51);
	for distance in 1..=70 {
		for count in [3, 4, 7, 15, 16, 17, 31, 32, 33, 64, 100, 255, 257, 258] {
			let to = distance + rng.below(20);
			let mut out = random(&mut rng, to + count + FAST_ROOM);
			let mut expected = out.clone();
			for i in to..to + count {
				expected[i] = expected[i - distance];
			}
			assert!(copy(&mut out, to, distance, count));
			assert_eq!(
				out[..to + count],
				expected[..to + count],
				"{count} bytes from {distance} back"
			);
		}
	}
}

#[test]
fn copies_write_less_than_a_vector_past_their_end() {
	// The room kept past the output covers what a copy overshoots.
	let mut rng = Rng::new(52);
	for distance in [1, 2, 3, 5, 7, 15, 16, 17, 40] {
		for count in [3, 16, 100, 258] {
			let to = 40;
			let mut out = random(&mut rng, to + count + FAST_ROOM);
			let after = out[to + count + 15..].to_vec();
			assert!(copy(&mut out, to, distance, count));
			assert_eq!(
				out[to + count + 15..],
				after,
				"{count} from {distance} back"
			);
		}
	}
	const { assert!(258 + 15 < FAST_ROOM) };
}

#[test]
fn copies_from_before_the_output_are_refused() {
	let mut out = vec![7; 64];
	assert!(!copy(&mut out, 3, 4, 3));
	assert!(!copy(&mut out, 0, 1, 3));
	assert!(!copy(&mut out, 10, 40, 16));
	assert_eq!(out, [7; 64]);
	assert!(copy(&mut out, 4, 4, 3));
}

/// Decodes `data`, compressed, with room for `expected` bytes up front,
/// and returns the room it ended with.
fn room(data: &[u8], expected: usize, limit: usize) -> Option<usize> {
	let stream = compress(data, 6);
	let mut inflater = Inflater::new(&stream[2..], expected, limit);
	inflater.run()?;
	assert_eq!(inflater.out[..inflater.len], *data);
	Some(inflater.out.len() - FAST_ROOM)
}

#[test]
fn enough_room_up_front_is_kept() {
	let data = sample(&mut Rng::new(53), 1, 5000);
	assert_eq!(room(&data, 5000, LIMIT), Some(5000));
	assert_eq!(room(&data, 8000, LIMIT), Some(8000));
}

#[test]
fn the_output_doubles_when_it_runs_out_of_room() {
	// Matches and literals need little room each, so every step doubles.
	let data = sample(&mut Rng::new(54), 1, 100_000);
	assert_eq!(room(&data, 1000, LIMIT), Some(128_000));
	assert_eq!(room(&data, 3000, LIMIT), Some(192_000));
	// The last symbol may end in the room kept past the output, with no
	// need to grow; `zlib` checks the limit afterwards.
	assert_eq!(room(&data, 99_999, LIMIT), Some(99_999));
}

#[test]
fn growth_stops_at_the_limit() {
	let data = sample(&mut Rng::new(55), 1, 100_000);
	assert_eq!(room(&data, 1000, 100_000), Some(100_000));
	assert_eq!(zlib(&compress(&data, 6), 1000, 99_999), None);
	assert_eq!(zlib(&compress(&data, 6), 99_999, 99_999), None);
	// A stored block larger than double the room grows to fit it at once.
	let data = random(&mut Rng::new(56), 60_000);
	let mut writer = Writer::default();
	stored(&mut writer, &data, true);
	let stream = writer.zlib(&data);
	let mut inflater = Inflater::new(&stream[2..], 10, LIMIT);
	assert_eq!(inflater.run(), Some(()));
	assert_eq!(inflater.out.len() - FAST_ROOM, 60_000);
}

#[test]
fn any_room_up_front_decodes_the_same() {
	// Where the fast loop hands over to the careful one depends on the
	// room, so try every amount around the output's size.
	let mut rng = Rng::new(57);
	for kind in 0..4 {
		let data = sample(&mut rng, kind, 700);
		let stream = compress(&data, 6);
		for expected in (0..1300).chain([LIMIT, usize::MAX]) {
			assert_eq!(
				zlib(&stream, expected, LIMIT).as_deref(),
				Some(&data[..]),
				"kind {kind}, {expected} up front"
			);
		}
	}
}

/// A code to write symbols in: each symbol's length and canonical code.
struct Code {
	lengths: Vec<u8>,
	codes: Vec<Option<u32>>,
}

impl Code {
	fn new(lengths: Vec<u8>) -> Self {
		let codes = canonical(&lengths);
		Self { lengths, codes }
	}

	fn put(&self, writer: &mut Writer, symbol: usize) {
		writer.code(self.codes[symbol].unwrap(), self.lengths[symbol].into());
	}
}

/// Writes `value` as the symbol of `table` it falls in, and its extra bits.
fn put_base(writer: &mut Writer, code: &Code, first: usize, table: &[(u16, u8)], value: usize) {
	let symbol = table
		.iter()
		.rposition(|&(base, _)| usize::from(base) <= value)
		.unwrap();
	let (base, extra) = table[symbol];
	code.put(writer, first + symbol);
	writer.bits((value - usize::from(base)) as u32, extra.into());
}

/// Writes random literals and matches in these codes, appending what they
/// decode to to `data`, then the end of the block.
fn content(
	rng: &mut Rng,
	writer: &mut Writer,
	data: &mut Vec<u8>,
	literals: &Code,
	distances: &Code,
) {
	for _ in 0..rng.below(300) {
		if data.is_empty() || rng.below(3) == 0 {
			let byte = rng.below(256);
			literals.put(writer, byte);
			data.push(byte as u8);
			continue;
		}
		// Short distances half the time, so periods are common.
		let length = 3 + rng.below(256);
		let reach = if rng.below(2) == 0 { 20 } else { 32_768 };
		let distance = 1 + rng.below(data.len().min(reach));
		put_base(writer, literals, 257, &LENGTHS, length);
		put_base(writer, distances, 0, &DISTANCES, distance);
		for _ in 0..length {
			data.push(data[data.len() - distance]);
		}
	}
	literals.put(writer, 256);
}

#[test]
fn random_sequences_of_blocks_decode_like_the_reference() {
	let mut rng = Rng::new(58);
	let mut fixed = vec![8u8; 288];
	fixed[144..256].fill(9);
	fixed[256..280].fill(7);
	let (fixed, fixed_distances) = (Code::new(fixed), Code::new(vec![5; 30]));
	for _ in 0..300 {
		let (mut writer, mut data) = (Writer::default(), Vec::new());
		let blocks = 1 + rng.below(5);
		for block in 0..blocks {
			let last = block + 1 == blocks;
			match rng.below(3) {
				0 => {
					let size = rng.below(1000);
					let bytes = random(&mut rng, size);
					stored(&mut writer, &bytes, last);
					data.extend_from_slice(&bytes);
				}
				1 => {
					writer.bits(0b010 | u32::from(last), 3);
					content(&mut rng, &mut writer, &mut data, &fixed, &fixed_distances);
				}
				_ => {
					// Every symbol has a code, of random lengths up to 15.
					let literals = Code::new(complete(&mut rng, 286, 286));
					let distances = Code::new(complete(&mut rng, 30, 30));
					dynamic_block(&mut writer, last, &literals.lengths, &distances.lengths);
					content(&mut rng, &mut writer, &mut data, &literals, &distances);
				}
			}
		}
		let stream = writer.zlib(&data);
		assert_eq!(agrees(&stream, LIMIT).as_ref(), Some(&data));
		assert_eq!(zlib(&stream, data.len(), data.len()), Some(data));
	}
}

#[test]
fn the_checksum_must_follow_and_match() {
	let data = b"a checksum covers the output";
	let stream = compress(data, 6);
	assert_eq!(agrees(&stream, LIMIT).as_deref(), Some(&data[..]));
	for bit in (stream.len() - 4) * 8..stream.len() * 8 {
		let mut damaged = stream.clone();
		damaged[bit / 8] ^= 1 << (bit % 8);
		assert_eq!(agrees(&damaged, LIMIT), None, "bit {bit}");
	}
	for cut in 1..=4 {
		assert_eq!(
			agrees(&stream[..stream.len() - cut], LIMIT),
			None,
			"{cut} cut"
		);
	}
	// The checksum is of the output, not of the compressed data.
	let mut writer = Writer::default();
	stored(&mut writer, data, true);
	assert_eq!(
		agrees(&writer.zlib(data), LIMIT).as_deref(),
		Some(&data[..])
	);
	assert_eq!(agrees(&writer.zlib(b"other"), LIMIT), None);
}
