//! Inflate tests, checked against `miniz_oxide` as the oracle.
use super::{ORDER, adler::adler32, zlib};
use crate::bench::corpus::Rng;

pub(super) const LIMIT: usize = 1 << 22;

pub(super) fn miniz(stream: &[u8], limit: usize) -> Option<Vec<u8>> {
	miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stream, limit).ok()
}

/// Asserts both decoders agree on `stream`, and returns the result.
pub(super) fn agrees(stream: &[u8], limit: usize) -> Option<Vec<u8>> {
	let ours = zlib(stream, 64, limit);
	assert_eq!(ours, miniz(stream, limit), "{stream:02x?}");
	ours
}

pub(super) fn compress(data: &[u8], level: u8) -> Vec<u8> {
	miniz_oxide::deflate::compress_to_vec_zlib(data, level)
}

/// Data of different kinds: random bytes, repeated words, long runs and
/// short periods, so every kind of block and match appears.
pub(super) fn sample(rng: &mut Rng, kind: usize, size: usize) -> Vec<u8> {
	let words: [&[u8]; 5] = [b"the ", b"quick ", b"brown fox ", b"\x1b[0m", b"\n"];
	let mut data = Vec::with_capacity(size);
	while data.len() < size {
		match kind {
			0 => data.push(rng.below(256) as u8),
			1 => data.extend_from_slice(rng.pick(&words)),
			2 => data.extend(std::iter::repeat_n(rng.below(4) as u8, 1 + rng.below(600))),
			_ => {
				let period = 2 + rng.below(14);
				let pattern: Vec<u8> = (0..period).map(|_| rng.below(256) as u8).collect();
				data.extend(pattern.iter().cycle().take(rng.below(300)));
			}
		}
	}
	data.truncate(size);
	data
}

#[test]
fn streams_at_every_level_round_trip() {
	let mut rng = Rng::new(11);
	for size in [0, 1, 2, 15, 16, 17, 258, 259, 1000, 32_768, 33_000, 200_000] {
		for kind in 0..4 {
			let data = sample(&mut rng, kind, size);
			for level in 0..=10 {
				let stream = compress(&data, level);
				let what = format!("kind {kind}, {size} bytes, level {level}");
				assert_eq!(
					zlib(&stream, data.len(), data.len()).as_ref(),
					Some(&data),
					"{what}"
				);
				assert_eq!(zlib(&stream, 0, LIMIT).as_ref(), Some(&data), "{what}");
			}
		}
	}
}

#[test]
fn damaged_streams_fail_like_the_reference() {
	let mut rng = Rng::new(5);
	let (mut decoded, mut refused) = (0, 0);
	for _ in 0..4000 {
		let (kind, size) = (rng.below(4), rng.below(3000));
		let mut stream = compress(&sample(&mut rng, kind, size), rng.below(11) as u8);
		match rng.below(4) {
			0 => stream.truncate(rng.below(stream.len() + 1)),
			1 => stream.extend((0..rng.below(8)).map(|_| rng.below(256) as u8)),
			_ => {
				// Damage past the header, or the checksum alone fails it.
				for _ in 0..1 + rng.below(3) {
					let bit = 16 + rng.below((stream.len() - 2) * 8);
					stream[bit / 8] ^= 1 << (bit % 8);
				}
			}
		}
		match agrees(&stream, LIMIT) {
			Some(_) => decoded += 1,
			None => refused += 1,
		}
	}
	// Both outcomes are common enough to have been compared.
	assert!(
		decoded > 1000 && refused > 1000,
		"{decoded} decoded, {refused} refused"
	);
}

#[test]
fn output_past_the_limit_is_refused() {
	let data = sample(&mut Rng::new(1), 1, 1000);
	for level in [0, 1, 6] {
		let stream = compress(&data, level);
		assert_eq!(agrees(&stream, 999), None);
		assert_eq!(agrees(&stream, 1000), Some(data.clone()));
	}
	// A bomb stops at the limit instead of filling memory.
	let bomb = compress(&vec![0; 1 << 24], 9);
	assert!(bomb.len() < 1 << 15);
	assert_eq!(agrees(&bomb, 1 << 16), None);
	assert_eq!(zlib(&bomb, 0, 1 << 24).map(|out| out.len()), Some(1 << 24));
}

/// Writes DEFLATE bits, least significant first.
#[derive(Default)]
pub(super) struct Writer {
	pub(super) bytes: Vec<u8>,
	pub(super) used: u32,
}

impl Writer {
	pub(super) fn bits(&mut self, value: u32, count: u32) -> &mut Self {
		for bit in 0..count {
			if self.used % 8 == 0 {
				self.bytes.push(0);
			}
			*self.bytes.last_mut().unwrap() |= ((value >> bit & 1) as u8) << (self.used % 8);
			self.used += 1;
		}
		self
	}

	/// A Huffman code, which is packed starting with its top bit.
	pub(super) fn code(&mut self, code: u32, count: u32) -> &mut Self {
		self.bits(code.reverse_bits() >> (32 - count), count)
	}

	/// A symbol of the fixed literal and length code.
	pub(super) fn fixed(&mut self, symbol: u32) -> &mut Self {
		match symbol {
			0..144 => self.code(0x30 + symbol, 8),
			144..256 => self.code(0x190 + symbol - 144, 9),
			256..280 => self.code(symbol - 256, 7),
			_ => self.code(0xc0 + symbol - 280, 8),
		}
	}

	/// The stream wrapped in a zlib header and the checksum of `data`.
	pub(super) fn zlib(&self, data: &[u8]) -> Vec<u8> {
		[&[0x78, 0x9c], &self.bytes[..], &adler32(data).to_be_bytes()].concat()
	}
}

#[test]
fn fixed_blocks_decode_literals_and_matches() {
	// "ab", then 7 bytes from 2 back (length symbol 261, distance symbol 1).
	let mut writer = Writer::default();
	writer.bits(0b011, 3).fixed(b'a'.into()).fixed(b'b'.into());
	writer.fixed(261).code(1, 5).fixed(256);
	assert_eq!(
		agrees(&writer.zlib(b"ababababa"), LIMIT).as_deref(),
		Some(&b"ababababa"[..])
	);
	// The same, with a mistaken checksum or a truncated one.
	assert_eq!(agrees(&writer.zlib(b"abababab"), LIMIT), None);
	let stream = writer.zlib(b"ababababa");
	assert_eq!(agrees(&stream[..stream.len() - 1], LIMIT), None);
	// Trailing input after the checksum is ignored.
	assert!(agrees(&[&stream[..], b"junk"].concat(), LIMIT).is_some());
}

#[test]
fn invalid_symbols_and_distances_are_refused() {
	let stream = |body: &dyn Fn(&mut Writer)| {
		let mut writer = Writer::default();
		writer.bits(0b011, 3).fixed(b'x'.into());
		body(&mut writer);
		writer.fixed(256).zlib(b"x")
	};
	// Length symbols 286 and 287 and distance symbols 30 and 31 have codes
	// in the fixed code, but no meaning.
	for symbol in [286, 287] {
		assert_eq!(agrees(&stream(&|w| _ = w.fixed(symbol)), LIMIT), None);
	}
	for symbol in [30, 31] {
		assert_eq!(
			agrees(&stream(&|w| _ = w.fixed(257).code(symbol, 5)), LIMIT),
			None
		);
	}
	// A distance reaching before the start of the output.
	assert_eq!(
		agrees(&stream(&|w| _ = w.fixed(257).code(1, 5)), LIMIT),
		None
	);
	// Block type 3 is reserved.
	let mut reserved = Writer::default();
	reserved.bits(0b111, 3);
	assert_eq!(agrees(&reserved.zlib(b""), LIMIT), None);
}

#[test]
fn stored_blocks_check_their_length() {
	let stored = |length: u16, check: u16, data: &[u8]| {
		let mut writer = Writer::default();
		writer.bits(1, 3).bits(0, 5);
		writer.bits(length.into(), 16).bits(check.into(), 16);
		writer.bytes.extend_from_slice(data);
		writer.zlib(data)
	};
	assert_eq!(
		agrees(&stored(3, !3, b"abc"), LIMIT).as_deref(),
		Some(&b"abc"[..])
	);
	assert_eq!(
		agrees(&stored(0, !0, b""), LIMIT).as_deref(),
		Some(&b""[..])
	);
	assert_eq!(agrees(&stored(3, !4, b"abc"), LIMIT), None);
	// Fewer bytes than the length says.
	let short = stored(3, !3, b"abc");
	assert_eq!(agrees(&[&short[..7], &short[9..]].concat(), LIMIT), None);
}

#[test]
fn zlib_headers_are_checked() {
	let good = compress(b"hello", 6);
	assert_eq!(agrees(&good, LIMIT).as_deref(), Some(&b"hello"[..]));
	let header = |method: u8, flags: u8| {
		// Fix the check bits, so only the field under test is wrong.
		let flags = flags & !31;
		let flags = flags + (31 - (u16::from(method) << 8 | u16::from(flags)) % 31) as u8 % 31;
		[&[method, flags], &good[2..]].concat()
	};
	assert_eq!(
		zlib(&header(0x78, 0x9c), 0, LIMIT).as_deref(),
		Some(&b"hello"[..])
	);
	// Another method, a window over 32 KiB, a preset dictionary.
	for (method, flags) in [(0x79, 0x9c), (0x88, 0x9c), (0x78, 0xbc)] {
		assert_eq!(
			agrees(&header(method, flags), LIMIT),
			None,
			"{method:x} {flags:x}"
		);
	}
	assert_eq!(agrees(&[0x78, 0x9d], LIMIT), None);
	assert_eq!(agrees(&[0x78], LIMIT), None);
	assert_eq!(agrees(&[], LIMIT), None);
}

/// A dynamic block header for these literal and distance code lengths,
/// each sent as itself: code lengths 0 to 12 have 4-bit codes, the rest 5.
pub(super) fn dynamic(writer: &mut Writer, literals: &[u8], distances: &[u8]) {
	dynamic_block(writer, true, literals, distances);
}

/// The same, for a block that may not be the last.
pub(super) fn dynamic_block(writer: &mut Writer, last: bool, literals: &[u8], distances: &[u8]) {
	writer.bits(0b100 | u32::from(last), 3);
	writer
		.bits(literals.len() as u32 - 257, 5)
		.bits(distances.len() as u32 - 1, 5);
	writer.bits(15, 4);
	for symbol in ORDER {
		writer.bits(if symbol < 13 { 4 } else { 5 }, 3);
	}
	for &length in literals.iter().chain(distances) {
		match u32::from(length) {
			length @ 0..13 => writer.code(length, 4),
			length => writer.code(26 + length - 13, 5),
		};
	}
}

#[test]
fn dynamic_code_lengths_are_checked() {
	let header = |literals: &[u8], distances: &[u8]| {
		let mut writer = Writer::default();
		dynamic(&mut writer, literals, distances);
		writer.zlib(b"")
	};
	// No end of block code.
	let mut literals = [0u8; 257];
	literals[0] = 1;
	literals[1] = 1;
	assert_eq!(agrees(&header(&literals, &[1]), LIMIT), None);
	// Three codes of one bit are more than one bit can tell apart.
	literals[256] = 1;
	assert_eq!(agrees(&header(&literals, &[1]), LIMIT), None);
	// Three codes of two bits leave a code unused.
	literals[..2].fill(2);
	literals[256] = 2;
	assert_eq!(agrees(&header(&literals, &[1]), LIMIT), None);
	// Codes that do not end at the input's end.
	assert_eq!(agrees(&header(&[0; 257], &[1])[..10], LIMIT), None);
}

#[test]
fn dynamic_blocks_decode_with_one_code_or_long_codes() {
	// Only the end of block code, of one bit, and no distance codes at all.
	let mut lengths = [0u8; 257];
	lengths[256] = 1;
	let mut writer = Writer::default();
	dynamic(&mut writer, &lengths, &[0]);
	writer.code(0, 1);
	assert_eq!(agrees(&writer.zlib(b""), LIMIT).as_deref(), Some(&b""[..]));
	// Codes of every length to 15 bits: symbol n gets n + 1 bits, the last
	// two 15. Symbol 14 is code 0b111...10 (15 bits), past the first lookup.
	let mut lengths = [0u8; 257];
	for (symbol, length) in lengths.iter_mut().take(15).enumerate() {
		*length = symbol as u8 + 1;
	}
	lengths[256] = 15;
	let mut writer = Writer::default();
	dynamic(&mut writer, &lengths, &[1]);
	let code = |symbol: u32| (1 << (symbol + 1)) - 2;
	for symbol in [14, 0, 13, 9, 10] {
		writer.code(code(symbol), symbol + 1);
	}
	writer.code(0x7fff, 15);
	let data = [14, 0, 13, 9, 10];
	assert_eq!(
		agrees(&writer.zlib(&data), LIMIT).as_deref(),
		Some(&data[..])
	);
}
