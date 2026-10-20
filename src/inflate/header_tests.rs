//! Block headers at their limits (dynamic code counts, stored lengths and
//! padding) and what each symbol of the fixed code means.
use super::{
	DISTANCES, LENGTHS, ORDER, PERIODS,
	blocks_tests::{decodes, stored},
	distance,
	huffman::{BASE, END, INVALID, KIND, LITERAL, extra},
	literal_or_length,
	tests::{LIMIT, Writer, agrees, dynamic},
};

/// Code lengths for `literals` and `distances` symbols where literal 0 and
/// the end of block have one bit each, and the first distance one bit.
fn lengths(literals: usize, distances: usize) -> (Vec<u8>, Vec<u8>) {
	let mut literal = vec![0; literals];
	(literal[0], literal[256]) = (1, 1);
	let mut distance = vec![0; distances];
	distance[0] = 1;
	(literal, distance)
}

#[test]
fn every_count_of_codes_the_rfc_allows_decodes() {
	for literals in 257..=286 {
		for distances in [1, 2, 29, 30] {
			let (literal, distance) = lengths(literals, distances);
			let mut writer = Writer::default();
			dynamic(&mut writer, &literal, &distance);
			writer.code(0, 1).code(1, 1);
			assert_eq!(
				agrees(&writer.zlib(&[0]), LIMIT).as_deref(),
				Some(&[0][..]),
				"{literals} literal and {distances} distance codes"
			);
		}
	}
}

#[test]
fn counts_past_the_rfc_are_refused() {
	// HLIT can say 287 or 288 and HDIST 31 or 32, but RFC 1951 allows at
	// most 286 and 30.
	for (literals, distances) in [(287, 1), (288, 1), (257, 31), (257, 32), (288, 32)] {
		let (literal, distance) = lengths(literals, distances);
		let mut writer = Writer::default();
		dynamic(&mut writer, &literal, &distance);
		writer.code(0, 1).code(1, 1);
		assert_eq!(
			agrees(&writer.zlib(&[0]), LIMIT),
			None,
			"{literals} literal and {distances} distance codes"
		);
	}
}

#[test]
fn fixed_symbols_mean_literals_the_end_and_lengths() {
	for symbol in 0..256 {
		assert_eq!(literal_or_length(symbol), LITERAL | (symbol as u32) << 16);
	}
	assert_eq!(literal_or_length(256), END);
	for symbol in 257..286 {
		let entry = literal_or_length(symbol);
		let (base, bits) = LENGTHS[symbol - 257];
		assert_eq!(entry & KIND, BASE);
		assert_eq!((entry >> 16, extra(entry)), (base.into(), bits.into()));
	}
	for symbol in [286, 287, 300] {
		assert_eq!(literal_or_length(symbol), INVALID);
	}
}

#[test]
fn distance_symbols_mean_their_base_and_extra_bits() {
	for (symbol, &(base, bits)) in DISTANCES.iter().enumerate() {
		let entry = distance(symbol);
		assert_eq!(entry & KIND, BASE);
		assert_eq!((entry >> 16, extra(entry)), (base.into(), bits.into()));
	}
	for symbol in [30, 31, 40] {
		assert_eq!(distance(symbol), INVALID);
	}
}

#[test]
fn every_fixed_literal_decodes() {
	// Codes of 8 bits up to 143 and of 9 bits from 144, in the fast loop
	// with room up front and per symbol without.
	let data: Vec<u8> = (0..=255).chain((0..=255).rev()).collect();
	let mut writer = Writer::default();
	writer.bits(0b011, 3);
	for &byte in &data {
		writer.fixed(byte.into());
	}
	writer.fixed(256);
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn stored_lengths_with_any_bit_of_the_check_flipped_are_refused() {
	for bit in 0..16 {
		let mut writer = Writer::default();
		writer.bits(1, 3).bits(0, 5);
		writer.bits(5, 16).bits(!5 ^ 1 << bit & 0xffff, 16);
		writer.bytes.extend_from_slice(b"hello");
		assert_eq!(agrees(&writer.zlib(b"hello"), LIMIT), None, "bit {bit}");
	}
}

#[test]
fn padding_before_stored_bytes_is_ignored() {
	// The bits after the block type up to the byte boundary are skipped
	// whatever they hold, after fixed codes of any length too.
	for (prefix, padding) in [
		(&b""[..], 0b11111),
		(b"a", 0b1),
		(b"ab", 0b101),
		(b"abc", 0),
	] {
		let mut writer = Writer::default();
		writer.bits(0b010, 3);
		for &byte in prefix {
			writer.fixed(byte.into());
		}
		writer.fixed(256);
		writer.bits(1, 3);
		let fill = (8 - writer.used % 8) % 8;
		writer.bits(padding & ((1 << fill) - 1), fill);
		writer.bits(3, 16).bits(!3 & 0xffff, 16);
		writer.bytes.extend_from_slice(b"xyz");
		writer.used += 24;
		let data = [prefix, b"xyz"].concat();
		decodes(&writer.zlib(&data), &data);
	}
}

#[test]
fn stored_blocks_of_the_largest_length_follow_each_other() {
	let mut rng = crate::bench::corpus::Rng::new(5);
	let data: Vec<u8> = (0..2 * 65_535 + 1).map(|_| rng.below(256) as u8).collect();
	let mut writer = Writer::default();
	stored(&mut writer, &data[..65_535], false);
	stored(&mut writer, &data[65_535..131_070], false);
	stored(&mut writer, &data[131_070..], true);
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn period_patterns_repeat_each_short_distance() {
	for (distance, lanes) in PERIODS.iter().enumerate().skip(1) {
		let repeated: Vec<u8> = (0..16).map(|lane| (lane % distance) as u8).collect();
		assert_eq!(&lanes[..], &repeated[..], "distance {distance}");
		// Every lane reads a byte already behind the copy.
		assert!(lanes.iter().all(|&lane| usize::from(lane) < distance));
	}
}

#[test]
fn distance_codes_past_the_first_lookup_decode() {
	// Distance symbol n has n + 1 bits up to 15, the last two 15, so
	// symbols 8 to 15 go through a subtable. Each copies three bytes from
	// a random history, from its smallest distance.
	let mut rng = crate::bench::corpus::Rng::new(9);
	let mut data: Vec<u8> = (0..300).map(|_| rng.below(256) as u8).collect();
	let mut writer = Writer::default();
	stored(&mut writer, &data, false);
	let mut literal = vec![0; 258];
	(literal[256], literal[257]) = (1, 1);
	let mut distance = [15; 16];
	for (symbol, length) in distance.iter_mut().take(15).enumerate() {
		*length = symbol as u8 + 1;
	}
	super::tests::dynamic_block(&mut writer, true, &literal, &distance);
	for (symbol, &(base, extra)) in DISTANCES.iter().enumerate().take(16) {
		let (code, length) = match symbol {
			15 => (0x7fff, 15),
			_ => ((1 << (symbol + 1)) - 2, symbol as u32 + 1),
		};
		writer.code(1, 1).code(code, length).bits(0, extra.into());
		for _ in 0..3 {
			data.push(data[data.len() - usize::from(base)]);
		}
	}
	writer.code(0, 1);
	decodes(&writer.zlib(&data), &data);
}

#[test]
fn every_window_size_and_level_in_the_header_is_accepted() {
	// CINFO 0 to 7 (windows of 256 bytes to 32 KiB) with any FLEVEL; the
	// window only bounds distances, which the stream checks anyway.
	let mut writer = Writer::default();
	writer.bits(0b011, 3).fixed(b'x'.into()).fixed(256);
	let body = writer.zlib(b"x");
	for window in 0..16u8 {
		for level in 0..4u8 {
			let method = window << 4 | 8;
			let flags = level << 6;
			let flags = flags + (31 - (u16::from(method) << 8 | u16::from(flags)) % 31) as u8 % 31;
			let stream = [&[method, flags], &body[2..]].concat();
			let expected = (window <= 7).then(|| b"x".to_vec());
			assert_eq!(super::zlib(&stream, 1, LIMIT), expected, "{window} {level}");
		}
	}
}

#[test]
fn output_ending_exactly_at_the_limit_is_accepted() {
	let data = b"abcabcabcabcabcabcabcabcabcabcabcabcabc";
	let fixed = super::tests::compress(data, 6);
	let mut writer = Writer::default();
	stored(&mut writer, data, true);
	let stored = writer.zlib(data);
	for stream in [fixed, stored] {
		for room in [0, data.len()] {
			let decode = |limit| super::zlib(&stream, room, limit);
			assert_eq!(decode(data.len()).as_deref(), Some(&data[..]));
			assert_eq!(decode(data.len() - 1), None, "room {room}");
		}
	}
}

#[test]
fn code_length_codes_left_out_of_the_header_have_no_code() {
	// HCLEN sends 18 of the 19 code length code lengths, enough to reach
	// length 1; only lengths 0 and 1 get a code (one bit each).
	let mut writer = Writer::default();
	writer.bits(0b101, 3).bits(0, 5).bits(0, 5).bits(14, 4);
	for symbol in &ORDER[..18] {
		writer.bits(u32::from(*symbol <= 1), 3);
	}
	// Literal 0, the end of block and distance 0 have one bit each.
	let lengths = (0..258).map(|i| u32::from(i == 0 || i == 256 || i == 257));
	for length in lengths {
		writer.code(length, 1);
	}
	writer.code(0, 1).code(1, 1);
	assert_eq!(agrees(&writer.zlib(&[0]), LIMIT).as_deref(), Some(&[0][..]));
}
