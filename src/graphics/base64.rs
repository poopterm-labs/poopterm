//! Streaming base64 decoding (standard alphabet, padding optional), as the
//! kitty graphics protocol sends payloads in chunks.
use std::simd::{
	Select, Simd, ToBytes,
	cmp::{SimdPartialEq, SimdPartialOrd},
	simd_swizzle,
};

const INVALID: u8 = 0xff;
const PAD: u8 = 0xfe;

/// Sextet value of every byte: 0..=63, [`PAD`] for `=`, else [`INVALID`].
const TABLE: [u8; 256] = {
	let mut table = [INVALID; 256];
	let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut index = 0;
	while index < 64 {
		table[alphabet[index] as usize] = index as u8;
		index += 1;
	}
	table[b'=' as usize] = PAD;
	table
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invalid;

/// Decodes base64 pushed in pieces of any length.
#[derive(Default)]
pub(crate) struct Decoder {
	/// Sextets of an incomplete quantum.
	held: [u8; 4],
	held_len: usize,
	/// Padding was seen; only more padding may follow.
	ended: bool,
}

impl Decoder {
	pub(crate) fn push(&mut self, mut input: &[u8], out: &mut Vec<u8>) -> Result<(), Invalid> {
		out.reserve(input.len() / 4 * 3 + 3);
		while !input.is_empty() {
			if self.held_len == 0 && !self.ended {
				let used = decode_quanta(input, out);
				input = &input[used..];
				if input.is_empty() {
					break;
				}
			}
			self.push_byte(input[0], out)?;
			input = &input[1..];
		}
		Ok(())
	}

	fn push_byte(&mut self, byte: u8, out: &mut Vec<u8>) -> Result<(), Invalid> {
		match TABLE[usize::from(byte)] {
			INVALID => Err(Invalid),
			PAD => {
				// Padding completes a quantum of two or three characters.
				if self.held_len < 2 && !self.ended {
					return Err(Invalid);
				}
				self.flush(out);
				self.ended = true;
				Ok(())
			}
			_ if self.ended => Err(Invalid),
			sextet => {
				self.held[self.held_len] = sextet;
				self.held_len += 1;
				if self.held_len == 4 {
					self.flush(out);
				}
				Ok(())
			}
		}
	}

	/// Writes the bytes of the held characters.
	fn flush(&mut self, out: &mut Vec<u8>) {
		let [a, b, c, d] = self.held.map(u32::from);
		let n = a << 18 | b << 12 | c << 6 | d;
		let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
		out.extend_from_slice(&bytes[..self.held_len.saturating_sub(1)]);
		self.held = [0; 4];
		self.held_len = 0;
	}

	/// Padding was seen, so the encoded data ended.
	pub(crate) fn padded(&self) -> bool {
		self.ended
	}

	/// Ends the input; a final quantum may lack its padding.
	pub(crate) fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), Invalid> {
		if self.held_len == 1 {
			return Err(Invalid);
		}
		self.flush(out);
		Ok(())
	}
}

/// Decodes whole 4-character quanta without padding; returns the bytes used.
#[inline]
fn decode_quanta(input: &[u8], out: &mut Vec<u8>) -> usize {
	let (blocks, _) = input.as_chunks::<16>();
	let mut used = 0;
	for block in blocks {
		let Some(bytes) = decode_block(block) else {
			break;
		};
		out.extend_from_slice(&bytes[..12]);
		used += 16;
	}
	let (quanta, _) = input[used..].as_chunks::<4>();
	for (index, quantum) in quanta.iter().enumerate() {
		let sextets = quantum.map(|byte| TABLE[usize::from(byte)]);
		if sextets.iter().any(|&sextet| sextet >= 64) {
			return used + index * 4;
		}
		let [a, b, c, d] = sextets.map(u32::from);
		let n = a << 18 | b << 12 | c << 6 | d;
		out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
	}
	used + quanta.len() * 4
}

/// Decodes 16 characters into 12 bytes (followed by 4 unused ones), or
/// `None` when one is padding or not in the alphabet.
#[inline(always)]
fn decode_block(block: &[u8; 16]) -> Option<[u8; 16]> {
	let chars = Simd::from_array(*block);
	let within =
		|low: u8, high: u8| chars.simd_ge(Simd::splat(low)) & chars.simd_le(Simd::splat(high));
	let (upper, lower, digit) = (within(b'A', b'Z'), within(b'a', b'z'), within(b'0', b'9'));
	let (plus, slash) = (
		chars.simd_eq(Simd::splat(b'+')),
		chars.simd_eq(Simd::splat(b'/')),
	);
	if !(upper | lower | digit | plus | slash).all() {
		return None;
	}
	// What to add to each character to get its sextet, modulo 256.
	let offset = |first: u8, sextet: u8| Simd::splat(sextet.wrapping_sub(first));
	let offsets = upper.select(
		offset(b'A', 0),
		lower.select(
			offset(b'a', 26),
			digit.select(
				offset(b'0', 52),
				plus.select(offset(b'+', 62), offset(b'/', 63)),
			),
		),
	);
	// Each little-endian u32 holds the sextets a, b, c, d of one quantum.
	let quanta = Simd::<u32, 4>::from_le_bytes(chars + offsets);
	let byte = Simd::splat(0xff);
	let bits =
		(quanta & byte) << 18 | (quanta >> 8 & byte) << 12 | (quanta >> 16 & byte) << 6 | quanta >> 24;
	// Each quantum's 24 bits, most significant byte first.
	let bytes = simd_swizzle!(
		bits.to_le_bytes(),
		[2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 3, 7, 11, 15]
	);
	Some(bytes.to_array())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bench::corpus::Rng;

	fn decode(input: &[u8]) -> Option<Vec<u8>> {
		let mut decoder = Decoder::default();
		let mut out = Vec::new();
		decoder.push(input, &mut out).ok()?;
		decoder.finish(&mut out).ok()?;
		Some(out)
	}

	#[test]
	fn decodes_the_rfc_4648_vectors_with_and_without_padding() {
		for (plain, encoded) in [
			("", ""),
			("f", "Zg=="),
			("fo", "Zm8="),
			("foo", "Zm9v"),
			("foob", "Zm9vYg=="),
			("fooba", "Zm9vYmE="),
			("foobar", "Zm9vYmFy"),
		] {
			assert_eq!(decode(encoded.as_bytes()).unwrap(), plain.as_bytes());
			let bare = encoded.trim_end_matches('=');
			assert_eq!(decode(bare.as_bytes()).unwrap(), plain.as_bytes());
		}
	}

	#[test]
	fn any_split_of_the_input_gives_the_same_bytes() {
		let mut rng = Rng::new(3);
		for len in 0..200 {
			let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
			let encoded = crate::bench::images::base64(&data);
			let mut decoder = Decoder::default();
			let mut out = Vec::new();
			let mut rest = &encoded[..];
			while !rest.is_empty() {
				let take = (1 + rng.below(70)).min(rest.len());
				decoder.push(&rest[..take], &mut out).unwrap();
				rest = &rest[take..];
			}
			decoder.finish(&mut out).unwrap();
			assert_eq!(out, data, "length {len}");
		}
	}

	#[test]
	fn rejects_invalid_characters_and_data_after_padding() {
		assert_eq!(decode(b"Zm9v!"), None);
		assert_eq!(decode(b"Zg==Zm9v"), None);
		assert_eq!(decode(b"Z"), None);
		assert_eq!(decode(b"Zm9v\n"), None);
	}

	#[test]
	fn every_invalid_byte_is_rejected_at_every_position() {
		let data: Vec<u8> = (0..60).map(|i| (i * 37) as u8).collect();
		let encoded = crate::bench::images::base64(&data);
		assert_eq!(decode(&encoded).unwrap(), data);
		let invalid = (0..=255u8).filter(|&byte| TABLE[usize::from(byte)] == INVALID);
		for byte in invalid {
			for position in 0..encoded.len() {
				let mut bad = encoded.clone();
				bad[position] = byte;
				assert_eq!(decode(&bad), None, "byte {byte} at {position}");
			}
		}
	}
}
