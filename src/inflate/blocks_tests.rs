//! Block decoding tests: every length and distance, each kind of block, the
//! output limit and malformed dynamic headers, against `miniz_oxide`.
use super::{
	DISTANCES, LENGTHS, ORDER,
	tests::{LIMIT, Writer, agrees, compress, dynamic, dynamic_block, miniz},
	zlib,
};
use crate::bench::corpus::Rng;

/// Appends `bytes` as fixed literal codes, and to `data`.
fn literals(writer: &mut Writer, data: &mut Vec<u8>, bytes: &[u8]) {
	for &byte in bytes {
		writer.fixed(byte.into());
	}
	data.extend_from_slice(bytes);
}

/// Appends a match of `length` bytes from `distance` back in fixed codes,
/// and repeats those bytes in `data`.
fn copy(writer: &mut Writer, data: &mut Vec<u8>, length: usize, distance: usize) {
	let symbol = LENGTHS
		.iter()
		.rposition(|&(base, _)| usize::from(base) <= length)
		.unwrap();
	let (base, extra) = LENGTHS[symbol];
	writer.fixed(257 + symbol as u32);
	writer.bits((length - usize::from(base)) as u32, extra.into());
	let symbol = DISTANCES
		.iter()
		.rposition(|&(base, _)| usize::from(base) <= distance)
		.unwrap();
	let (base, extra) = DISTANCES[symbol];
	writer.code(symbol as u32, 5);
	writer.bits((distance - usize::from(base)) as u32, extra.into());
	for _ in 0..length {
		data.push(data[data.len() - distance]);
	}
}

/// Appends a stored block of `bytes`.
pub(super) fn stored(writer: &mut Writer, bytes: &[u8], last: bool) {
	writer.bits(u32::from(last), 3);
	writer.bits(0, (8 - writer.used % 8) % 8);
	let length = bytes.len() as u32;
	writer.bits(length, 16).bits(!length, 16);
	writer.bytes.extend_from_slice(bytes);
	writer.used += 8 * length;
}

/// Checks `stream` decodes to `data` with all the room up front, with none,
/// and in the reference.
pub(super) fn decodes(stream: &[u8], data: &[u8]) {
	assert_eq!(miniz(stream, LIMIT).as_deref(), Some(data), "reference");
	assert_eq!(
		zlib(stream, data.len(), data.len()).as_deref(),
		Some(data),
		"room up front"
	);
	assert_eq!(zlib(stream, 0, LIMIT).as_deref(), Some(data), "growing");
}

#[test]
fn length_and_distance_tables_match_the_rfc() {
	// RFC 1951, section 3.2.5.
	let lengths: [(u16, u8); 29] = [
		(3, 0),
		(4, 0),
		(5, 0),
		(6, 0),
		(7, 0),
		(8, 0),
		(9, 0),
		(10, 0),
		(11, 1),
		(13, 1),
		(15, 1),
		(17, 1),
		(19, 2),
		(23, 2),
		(27, 2),
		(31, 2),
		(35, 3),
		(43, 3),
		(51, 3),
		(59, 3),
		(67, 4),
		(83, 4),
		(99, 4),
		(115, 4),
		(131, 5),
		(163, 5),
		(195, 5),
		(227, 5),
		(258, 0),
	];
	let distances: [(u16, u8); 30] = [
		(1, 0),
		(2, 0),
		(3, 0),
		(4, 0),
		(5, 1),
		(7, 1),
		(9, 2),
		(13, 2),
		(17, 3),
		(25, 3),
		(33, 4),
		(49, 4),
		(65, 5),
		(97, 5),
		(129, 6),
		(193, 6),
		(257, 7),
		(385, 7),
		(513, 8),
		(769, 8),
		(1025, 9),
		(1537, 9),
		(2049, 10),
		(3073, 10),
		(4097, 11),
		(6145, 11),
		(8193, 12),
		(12289, 12),
		(16385, 13),
		(24577, 13),
	];
	assert_eq!(LENGTHS, lengths);
	assert_eq!(DISTANCES, distances);
}

#[test]
fn every_length_decodes_with_its_least_and_most_extra_bits() {
	for (symbol, &(base, _)) in LENGTHS.iter().enumerate() {
		let last = LENGTHS.get(symbol + 1).map_or(258, |&(next, _)| next - 1);
		for length in [base, last] {
			let (mut writer, mut data) = (Writer::default(), Vec::new());
			writer.bits(0b011, 3);
			literals(&mut writer, &mut data, b"0123456789abcdef");
			copy(&mut writer, &mut data, length.into(), 1 + symbol % 16);
			literals(&mut writer, &mut data, b"!");
			writer.fixed(256);
			decodes(&writer.zlib(&data), &data);
		}
	}
}

#[test]
fn every_distance_decodes_with_its_least_and_most_extra_bits() {
	let mut rng = Rng::new(41);
	let history: Vec<u8> = (0..32_768).map(|_| rng.below(256) as u8).collect();
	let mut writer = Writer::default();
	stored(&mut writer, &history, false);
	let mut data = history;
	writer.bits(0b011, 3);
	for (symbol, &(base, _)) in DISTANCES.iter().enumerate() {
		let last = DISTANCES
			.get(symbol + 1)
			.map_or(32_768, |&(next, _)| next - 1);
		for distance in [base, last] {
			copy(&mut writer, &mut data, 3 + symbol, distance.into());
		}
	}
	writer.fixed(256);
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn short_distances_repeat_their_period() {
	// The vector copies spread a period over 16 bytes: check each period
	// with copies shorter than, around and past a vector.
	let mut rng = Rng::new(42);
	for distance in 1..=40 {
		let (mut writer, mut data) = (Writer::default(), Vec::new());
		writer.bits(0b011, 3);
		let start: Vec<u8> = (0..40).map(|_| rng.below(256) as u8).collect();
		literals(&mut writer, &mut data, &start);
		for length in [3, 4, 15, 16, 17, 31, 32, 33, 47, 100, 257, 258] {
			copy(&mut writer, &mut data, length, distance);
		}
		writer.fixed(256);
		decodes(&writer.zlib(&data), &data);
	}
}

#[test]
fn matches_ending_past_the_limit_are_refused() {
	for length in [3, 10, 100, 258] {
		let (mut writer, mut data) = (Writer::default(), Vec::new());
		writer.bits(0b011, 3);
		literals(&mut writer, &mut data, b"abcdefghij");
		copy(&mut writer, &mut data, length, 1);
		writer.fixed(256);
		let stream = writer.zlib(&data);
		let total = data.len();
		assert_eq!(zlib(&stream, total, total).as_deref(), Some(&data[..]));
		// The match is decoded whole before the limit is checked.
		for over in [1, 2, length] {
			let limit = total - over;
			assert_eq!(miniz(&stream, limit), None);
			assert_eq!(zlib(&stream, limit, limit), None, "{over} past");
			assert_eq!(zlib(&stream, 0, limit), None, "{over} past");
		}
	}
}

#[test]
fn long_literal_codes_decode_in_the_fast_loop() {
	// Symbol n has n + 1 bits up to 15, so literals 10 to 14 are past the
	// primary table, and the output is long enough for the fast loop.
	let mut lengths = [0u8; 257];
	for (symbol, length) in lengths.iter_mut().take(15).enumerate() {
		*length = symbol as u8 + 1;
	}
	lengths[256] = 15;
	let mut writer = Writer::default();
	dynamic(&mut writer, &lengths, &[1]);
	let mut rng = Rng::new(43);
	let data: Vec<u8> = (0..3000).map(|_| 9 + rng.below(6) as u8).collect();
	for &symbol in &data {
		let symbol = u32::from(symbol);
		writer.code((1 << (symbol + 1)) - 2, symbol + 1);
	}
	writer.code(0x7fff, 15);
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn blocks_of_every_kind_follow_each_other() {
	let (mut writer, mut data) = (Writer::default(), Vec::new());
	stored(&mut writer, b"abc", false);
	data.extend_from_slice(b"abc");
	writer.bits(0b010, 3);
	literals(&mut writer, &mut data, b"def");
	writer.fixed(256);
	// A dynamic code: the end of block in one bit, literal b in nine as
	// 256 + b.
	let mut lengths = [9u8; 257];
	lengths[256] = 1;
	dynamic_block(&mut writer, false, &lengths, &[1]);
	for &byte in b"ghi" {
		writer.code(256 + u32::from(byte), 9);
	}
	data.extend_from_slice(b"ghi");
	writer.code(0, 1);
	// Fixed codes again, after the dynamic ones replaced them, reaching
	// back into the stored block; then an empty stored block.
	writer.bits(0b010, 3);
	copy(&mut writer, &mut data, 6, 9);
	writer.fixed(256);
	stored(&mut writer, b"", false);
	stored(&mut writer, b"mno", true);
	data.extend_from_slice(b"mno");
	assert_eq!(data, b"abcdefghiabcdefmno");
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn streams_without_a_last_block_are_refused() {
	let (mut writer, mut data) = (Writer::default(), Vec::new());
	writer.bits(0b010, 3);
	literals(&mut writer, &mut data, b"abc");
	writer.fixed(256);
	assert_eq!(agrees(&writer.zlib(&data), LIMIT), None);
	assert_eq!(agrees(&writer.bytes, LIMIT), None);
}

#[test]
fn stored_blocks_grow_the_output_up_to_the_limit() {
	let mut rng = Rng::new(44);
	let (mut writer, mut data) = (Writer::default(), Vec::new());
	for last in [false, false, true] {
		let block: Vec<u8> = (0..65_535).map(|_| rng.below(256) as u8).collect();
		stored(&mut writer, &block, last);
		data.extend_from_slice(&block);
	}
	let stream = writer.zlib(&data);
	decodes(&stream, &data);
	assert_eq!(zlib(&stream, 0, data.len() - 1), None);
	assert_eq!(zlib(&stream, 1000, 65_535 * 2), None);
}

#[test]
fn room_up_front_is_clamped_to_the_limit() {
	let data = b"hello hello hello";
	let stream = compress(data, 6);
	assert_eq!(zlib(&stream, usize::MAX, 100).as_deref(), Some(&data[..]));
	assert_eq!(
		zlib(&stream, 1 << 20, data.len()).as_deref(),
		Some(&data[..])
	);
	assert_eq!(zlib(&stream, usize::MAX, 5), None);
	// Less room than the output needs grows it.
	assert_eq!(zlib(&stream, 1, LIMIT).as_deref(), Some(&data[..]));
}

/// A dynamic block header whose code lengths are sent as these code length
/// symbols, each with its extra bits, in the code `dynamic` uses.
fn runs(writer: &mut Writer, literals: u32, distances: u32, symbols: &[(u32, u32)]) {
	writer
		.bits(0b101, 3)
		.bits(literals - 257, 5)
		.bits(distances - 1, 5);
	writer.bits(15, 4);
	for symbol in ORDER {
		writer.bits(if symbol < 13 { 4 } else { 5 }, 3);
	}
	for &(symbol, extra) in symbols {
		match symbol {
			0..13 => writer.code(symbol, 4),
			_ => writer.code(26 + symbol - 13, 5),
		};
		let count = match symbol {
			16 => 2,
			17 => 3,
			18 => 7,
			_ => 0,
		};
		writer.bits(extra, count);
	}
}

#[test]
fn runs_of_code_lengths_fill_the_list() {
	// Literal 0 and the end of block in one bit each, with 255 zeros
	// between them as runs of 138 and 117, and one distance code.
	let mut writer = Writer::default();
	runs(
		&mut writer,
		257,
		1,
		&[(1, 0), (18, 127), (18, 106), (1, 0), (1, 0)],
	);
	writer.code(0, 1).code(0, 1).code(0, 1).code(1, 1);
	decodes(&writer.zlib(&[0; 3]), &[0; 3]);
	// The same zeros as runs of 3 to 10 and 11 to 138.
	let mut writer = Writer::default();
	let zeros = [(17, 0), (17, 7), (18, 0), (18, 127), (17, 4), (18, 75)];
	let symbols: Vec<(u32, u32)> = [(1, 0)]
		.into_iter()
		.chain(zeros)
		.chain([(1, 0), (1, 0)])
		.collect();
	runs(&mut writer, 257, 1, &symbols);
	writer.code(0, 1).code(1, 1);
	decodes(&writer.zlib(&[0]), &[0]);
}

#[test]
fn bad_runs_of_code_lengths_are_refused() {
	let refused = |symbols: &[(u32, u32)]| {
		let mut writer = Writer::default();
		runs(&mut writer, 257, 1, symbols);
		writer.code(0, 1).code(1, 1);
		assert_eq!(agrees(&writer.zlib(&[0]), LIMIT), None, "{symbols:?}");
	};
	// Repeating the previous length with none before it.
	refused(&[(16, 0), (1, 0)]);
	// Runs past the end of the list.
	refused(&[(1, 0), (18, 127), (18, 127), (1, 0)]);
	refused(&[(1, 0), (18, 127), (18, 106), (1, 0), (16, 0)]);
}

#[test]
fn code_length_symbols_without_a_code_are_refused() {
	// Only code length 0 has a code (one bit, 0), so a 1 bit is no symbol.
	let header = |data: u32| {
		let mut writer = Writer::default();
		writer.bits(0b101, 3).bits(0, 5).bits(0, 5).bits(0, 4);
		writer.bits(0, 3).bits(0, 3).bits(0, 3).bits(1, 3);
		writer.bits(data, 1);
		writer.zlib(b"")
	};
	assert_eq!(agrees(&header(1), LIMIT), None);
	// With zeros, no code decodes anything: the block cannot even end.
	assert_eq!(agrees(&header(0), LIMIT), None);
}

#[test]
fn matches_without_distance_codes_are_refused() {
	// Literal 0 and the end of block in two bits, length symbol 257 in
	// one, and no distance codes at all.
	let mut lengths = [0u8; 258];
	(lengths[0], lengths[256], lengths[257]) = (2, 2, 1);
	let block = |writer: &mut Writer| {
		dynamic(writer, &lengths, &[0]);
		writer.code(0b10, 2);
	};
	let mut writer = Writer::default();
	block(&mut writer);
	writer.code(0b11, 2);
	decodes(&writer.zlib(&[0]), &[0]);
	let mut writer = Writer::default();
	block(&mut writer);
	writer.code(0, 1).code(0, 5).code(0b11, 2);
	assert_eq!(agrees(&writer.zlib(&[0; 4]), LIMIT), None);
}

#[test]
fn repeating_before_any_length_is_refused() {
	// Read as three zeros, the list would be complete: literal 3 and the end
	// of block in one bit each, and one distance code.
	let mut writer = Writer::default();
	let zeros = [(18, 127), (18, 103)];
	let symbols: Vec<(u32, u32)> = [(16, 0), (1, 0)]
		.into_iter()
		.chain(zeros)
		.chain([(1, 0), (1, 0)])
		.collect();
	runs(&mut writer, 257, 1, &symbols);
	writer.code(0, 1).code(1, 1);
	assert_eq!(agrees(&writer.zlib(&[3]), LIMIT), None);
	// The same list with a real length first decodes.
	let mut writer = Writer::default();
	let symbols: Vec<(u32, u32)> = [(0, 0), (0, 0), (0, 0), (1, 0)]
		.into_iter()
		.chain(zeros)
		.chain([(1, 0), (1, 0)])
		.collect();
	runs(&mut writer, 257, 1, &symbols);
	writer.code(0, 1).code(1, 1);
	decodes(&writer.zlib(&[3]), &[3]);
}

#[test]
fn distances_before_the_output_are_refused_near_the_end() {
	// Without room for the fast loop, the match after the first byte goes
	// through the careful path. The checksum is that of the output a
	// decoder would make by clamping the distance, so only the distance
	// check refuses the stream.
	for distance in 1..=4 {
		let mut writer = Writer::default();
		writer.bits(0b011, 3);
		writer
			.fixed(b'a'.into())
			.fixed(257)
			.code(distance - 1, 5)
			.fixed(256);
		let stream = writer.zlib(b"aaaa");
		let expected = (distance == 1).then(|| b"aaaa".to_vec());
		assert_eq!(zlib(&stream, 0, LIMIT), expected, "distance {distance}");
		assert_eq!(miniz(&stream, LIMIT), expected, "distance {distance}");
		// With room up front, the fast loop checks it too.
		assert_eq!(zlib(&stream, 4, 4), expected, "distance {distance}");
	}
}
