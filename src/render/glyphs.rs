//! Rasterized-glyph cache keys.
use std::collections::HashMap;

use fontdue::Metrics;

/// Rasterized glyphs keyed by (character, pixel size bits, font index).
pub(crate) type GlyphCache = HashMap<(char, u32, usize), (Metrics, Vec<u8>)>;

pub(crate) fn glyph_cache_key(ch: char, px: f32) -> (char, u32, usize) {
	glyph_cache_key_for(ch, px, 0)
}

pub(crate) fn glyph_cache_key_for(ch: char, px: f32, font_index: usize) -> (char, u32, usize) {
	(ch, px.to_bits(), font_index)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn glyphs_are_cached_separately_at_each_font_size() {
		assert_ne!(glyph_cache_key('a', 12.0), glyph_cache_key('a', 18.0));
		assert_eq!(glyph_cache_key('a', 12.0), glyph_cache_key('a', 12.0));
	}
}
