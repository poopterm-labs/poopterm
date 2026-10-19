//! Font fallback resolution and the rasterized-glyph cache.
use crate::fonts::{Font, Metrics};

use super::fxhash::FxHashMap;

/// Upper bound on cached rasterizations before the cache is reset.
const MAX_CACHED_GLYPHS: usize = 4096;

/// A rasterized glyph: placement metrics plus an 8-bit coverage bitmap.
pub(crate) struct Glyph {
	pub metrics: Metrics,
	pub coverage: Vec<u8>,
}

/// The primary font, its fallbacks, and every glyph rasterized so far.
///
/// Lookups are keyed by `(char, pixel size)` and resolve the fallback font
/// only on a cache miss, so drawing a cell costs a single hash lookup.
pub(crate) struct FontSet {
	primary: Font,
	fallbacks: Vec<Font>,
	cache: FxHashMap<(char, u32), Glyph>,
}

impl FontSet {
	pub(crate) fn new(primary: Font, fallbacks: Vec<Font>) -> Self {
		Self {
			primary,
			fallbacks,
			cache: FxHashMap::default(),
		}
	}

	pub(crate) fn primary(&self) -> &Font {
		&self.primary
	}

	/// Returns the cached glyph for `ch` at `px`, rasterizing it on first use
	/// with the first font that contains it (the primary font otherwise).
	#[inline]
	pub(crate) fn glyph(&mut self, ch: char, px: f32) -> &Glyph {
		let (primary, fallbacks) = (&self.primary, &self.fallbacks);
		self.cache.entry((ch, px.to_bits())).or_insert_with(|| {
			let font = if primary.lookup_glyph_index(ch) != 0 {
				primary
			} else {
				fallbacks
					.iter()
					.find(|font| font.lookup_glyph_index(ch) != 0)
					.unwrap_or(primary)
			};
			let (metrics, coverage) = font.rasterize(ch, px);
			Glyph { metrics, coverage }
		})
	}

	/// Bounds memory use; call once per frame, before drawing.
	pub(crate) fn trim(&mut self) {
		if self.cache.len() > MAX_CACHED_GLYPHS {
			self.cache.clear();
		}
	}

	/// Drops every rasterization, e.g. after the font size changes.
	pub(crate) fn clear(&mut self) {
		self.cache.clear();
	}

	#[cfg(test)]
	pub(crate) fn cached(&self) -> usize {
		self.cache.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bundled() -> Font {
		Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap()
	}

	#[test]
	fn glyphs_are_cached_separately_at_each_font_size() {
		let mut fonts = FontSet::new(bundled(), Vec::new());
		let small = fonts.glyph('a', 12.0).metrics.height;
		let large = fonts.glyph('a', 24.0).metrics.height;
		assert!(large > small);
		fonts.glyph('a', 12.0);
		assert_eq!(fonts.cached(), 2);
	}

	#[test]
	fn trim_resets_an_oversized_cache() {
		let mut fonts = FontSet::new(bundled(), Vec::new());
		for ch in (0..=MAX_CACHED_GLYPHS as u32).filter_map(char::from_u32) {
			fonts.glyph(ch, 10.0);
		}
		fonts.trim();
		assert_eq!(fonts.cached(), 0);
	}
}
