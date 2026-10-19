//! Validated UTF-8 decoding and common terminal character widths.
use unicode_width::UnicodeWidthChar;

/// Decodes one scalar, checking continuation bytes, shortest form, and range.
/// Invalid input stays on vte's path so replacement/control semantics agree.
#[inline]
pub(super) fn decode(bytes: &[u8]) -> Option<(char, usize)> {
	let &lead = bytes.first()?;
	if lead < 0x80 {
		return Some((lead as char, 1));
	}
	let (len, mut value, minimum) = match lead {
		0xc2..=0xdf => (2, u32::from(lead & 0x1f), 0x80),
		0xe0..=0xef => (3, u32::from(lead & 0x0f), 0x800),
		0xf0..=0xf4 => (4, u32::from(lead & 7), 0x10000),
		_ => return None,
	};
	for &byte in bytes.get(1..len)? {
		if byte & 0xc0 != 0x80 {
			return None;
		}
		value = (value << 6) | u32::from(byte & 0x3f);
	}
	if value < minimum {
		return None;
	}
	char::from_u32(value).map(|c| (c, len))
}

/// Common ranges avoid the general Unicode property lookup. Keep the
/// exhaustive scalar comparison below when updating unicode-width.
#[inline]
pub(super) fn width(c: char) -> usize {
	match c {
		' '..='~'
		| '\u{a0}'..='\u{ac}'
		| '\u{ae}'..='\u{2ff}'
		| '\u{370}'..='\u{3ff}'
		| '\u{2190}'..='\u{21ff}'
		| '\u{2500}'..='\u{257f}' => 1,
		'\u{4e00}'..='\u{9fff}' | '\u{1f600}'..='\u{1f64f}' => 2,
		_ => c.width().unwrap_or(0),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use unicode_width::UnicodeWidthChar;

	#[test]
	fn decodes_every_unicode_scalar_and_rejects_truncated_characters() {
		let mut buffer = [0; 4];
		for c in (0..=0x10ffff).filter_map(char::from_u32) {
			let bytes = c.encode_utf8(&mut buffer).as_bytes();
			assert_eq!(decode(bytes), Some((c, bytes.len())));
			for end in 0..bytes.len() {
				assert_eq!(decode(&bytes[..end]), None);
			}
			assert_eq!(width(c), c.width().unwrap_or(0));
		}
	}

	#[test]
	fn rejects_overlong_surrogate_out_of_range_and_non_continuation_bytes() {
		for bytes in [
			&b"\xc0\x80"[..],
			&b"\xc1\xbf"[..],
			&b"\xe0\x9f\xbf"[..],
			&b"\xed\xa0\x80"[..],
			&b"\xf0\x8f\xbf\xbf"[..],
			&b"\xf4\x90\x80\x80"[..],
			&b"\xf5\x80\x80\x80"[..],
			&b"\xe7x\xab"[..],
			&b"\xff"[..],
			&b"\x80"[..],
		] {
			assert_eq!(decode(bytes), None, "{bytes:?}");
		}
	}
}
