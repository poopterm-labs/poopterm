//! Terminal character widths, with a fast path for common ranges.
use unicode_width::UnicodeWidthChar;

/// Common ranges avoid the general Unicode property lookup. Keep the
/// exhaustive scalar comparison below when updating unicode-width.
#[inline]
pub(super) fn width(c: char) -> usize {
	match c {
		' '..='~'
		| '\u{a0}'..='\u{ac}'
		| '\u{ae}'..='\u{2ff}'
		| '\u{370}'..='\u{3ff}'
		| '\u{400}'..='\u{482}'
		| '\u{48a}'..='\u{52f}'
		| '\u{2010}'..='\u{2027}'
		| '\u{2190}'..='\u{21ff}'
		| '\u{2500}'..='\u{257f}' => 1,
		'\u{4e00}'..='\u{9fff}' | '\u{1f600}'..='\u{1f64f}' => 2,
		_ => c.width().unwrap_or(0),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn common_ranges_agree_with_unicode_width() {
		for c in (0..=0x10ffff).filter_map(char::from_u32) {
			assert_eq!(width(c), c.width().unwrap_or(0), "{c:?}");
		}
	}
}
