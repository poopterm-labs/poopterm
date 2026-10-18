//! Rectangle, button and text rasterization into a 0RGB pixel buffer.
use super::{
	blend::{blend, blend_coverage_row},
	glyphs::{FontSet, Glyph},
};

/// Fills a clipped rectangle with a solid color.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_rect(
	px: &mut [u32],
	w: usize,
	h: usize,
	x: usize,
	y: usize,
	rw: usize,
	rh: usize,
	color: u32,
) {
	let x_end = x.saturating_add(rw).min(w);
	if x >= x_end {
		return;
	}
	let height_end = y.saturating_add(rh).min(h);
	for yy in y..height_end {
		let row_end = yy * w + x_end;
		if row_end > px.len() {
			break;
		}
		px[yy * w + x..row_end].fill(color);
	}
}

/// Colors for a bordered, rounded button.
#[derive(Clone, Copy)]
pub(crate) struct ButtonStyle {
	pub scale: usize,
	pub background: u32,
	pub border: u32,
	pub accent: u32,
}

/// A borrowed frame buffer with drawing helpers. Coordinates are physical
/// pixels; everything is clipped to the buffer.
pub(crate) struct Painter<'a> {
	pub pixels: &'a mut [u32],
	pub width: usize,
	pub height: usize,
}

impl<'a> Painter<'a> {
	pub(crate) fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
		debug_assert!(pixels.len() >= width * height);
		Self {
			pixels,
			width,
			height,
		}
	}

	pub(crate) fn fill(&mut self, color: u32) {
		self.pixels.fill(color);
	}

	pub(crate) fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		fill_rect(self.pixels, self.width, self.height, x, y, w, h, color);
	}

	#[allow(clippy::too_many_arguments)]
	pub(crate) fn round_rect(
		&mut self,
		x: usize,
		y: usize,
		w: usize,
		h: usize,
		radius: usize,
		color: u32,
	) {
		let radius = radius.min(h / 2).min(w / 2);
		for row in 0..h {
			let dy = if row < radius {
				radius - row
			} else if row >= h - radius {
				row - (h - radius - 1)
			} else {
				0
			};
			let inset = if dy == 0 {
				0
			} else {
				radius.saturating_sub((radius * radius).saturating_sub(dy * dy).isqrt())
			};
			self.rect(x + inset, y + row, w.saturating_sub(2 * inset), 1, color);
		}
	}

	pub(crate) fn button(
		&mut self,
		(x, y, w, h): (usize, usize, usize, usize),
		style: ButtonStyle,
		hovered: bool,
	) {
		let s = style.scale;
		let (outline, face) = if hovered {
			(style.accent, blend(style.background, style.accent, 52))
		} else {
			(
				blend(style.background, style.border, 90),
				blend(style.background, style.border, 38),
			)
		};
		self.round_rect(x, y, w, h, 7 * s, outline);
		self.round_rect(
			x + s,
			y + s,
			w.saturating_sub(2 * s),
			h.saturating_sub(2 * s),
			6 * s,
			face,
		);
	}

	/// Draws a line of text whose top edge is `y`; `ascent` is the baseline offset.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text(
		&mut self,
		fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
	) {
		let mut cursor_x = x as i32;
		for ch in text.chars() {
			let glyph = fonts.glyph(ch, px);
			let origin = (cursor_x + glyph.metrics.xmin, glyph_top(glyph, y, ascent));
			self.glyph(glyph, origin, (0, self.width as i32), false, color);
			cursor_x += glyph.metrics.advance_width.ceil() as i32;
			if cursor_x >= self.width as i32 {
				break;
			}
		}
	}

	/// Blends `glyph` with its top-left corner at `(x, y)`, clipped horizontally
	/// to `clip = (left, right)`. `italic` shears rows to simulate slanted text.
	pub(crate) fn glyph(
		&mut self,
		glyph: &Glyph,
		(x, y): (i32, i32),
		(clip_left, clip_right): (i32, i32),
		italic: bool,
		color: u32,
	) {
		let metrics = &glyph.metrics;
		let top = y.max(0);
		let bottom = (y + metrics.height as i32).min(self.height as i32);
		let clip_left = clip_left.max(0);
		let clip_right = clip_right.min(self.width as i32);
		for py in top..bottom {
			let gy = (py - y) as usize;
			let shift = if italic {
				(metrics.height.saturating_sub(gy) / 6) as i32
			} else {
				0
			};
			let left = x + shift;
			let draw_left = left.max(clip_left);
			let draw_right = (left + metrics.width as i32).min(clip_right);
			if draw_left >= draw_right {
				continue;
			}
			let len = (draw_right - draw_left) as usize;
			let src = gy * metrics.width + (draw_left - left) as usize;
			let dst = py as usize * self.width + draw_left as usize;
			blend_coverage_row(
				&mut self.pixels[dst..dst + len],
				&glyph.coverage[src..src + len],
				color,
			);
		}
	}
}

/// Y coordinate of a glyph bitmap's top row for a line whose top edge is `y`.
#[inline]
pub(crate) fn glyph_top(glyph: &Glyph, y: usize, ascent: i32) -> i32 {
	y as i32 + ascent - glyph.metrics.ymin - glyph.metrics.height as i32
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rectangles_are_clipped_to_the_buffer() {
		let mut pixels = vec![0u32; 4 * 3];
		let mut painter = Painter::new(&mut pixels, 4, 3);
		painter.rect(2, 1, 10, 10, 7);
		assert_eq!(pixels, [0, 0, 0, 0, 0, 0, 7, 7, 0, 0, 7, 7]);
	}

	#[test]
	fn rounded_rectangles_inset_their_corners() {
		let mut pixels = vec![0u32; 8 * 8];
		Painter::new(&mut pixels, 8, 8).round_rect(0, 0, 8, 8, 3, 1);
		assert_eq!(pixels[0], 0, "corner stays clear");
		assert_eq!(pixels[4 * 8], 1, "middle row is filled edge to edge");
	}
}
