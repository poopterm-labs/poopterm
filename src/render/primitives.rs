//! Rectangle, button and text rasterization into a 0RGB pixel buffer.
use fontdue::Font;

use super::{
	blend::{blend, blend_coverage_row},
	glyphs::{GlyphCache, glyph_cache_key},
};

// ---- SIMD Accelerated Graphics & Blending Utilities -------------------------

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
	let color_array: [u32; 4] = [color; 4];
	let height_end = y.saturating_add(rh).min(h);

	for yy in y..height_end {
		let row_start = yy * w + x;
		let row_end = yy * w + x_end;
		if row_end > px.len() {
			break;
		}
		let row_slice = &mut px[row_start..row_end];

		let (chunks, rem) = row_slice.as_chunks_mut::<4>();
		for chunk in chunks {
			*chunk = color_array;
		}
		rem.fill(color);
	}
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_round_rect(
	pixels: &mut [u32],
	width: usize,
	height: usize,
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
			radius.saturating_sub(
				radius
					.saturating_mul(radius)
					.saturating_sub(dy * dy)
					.isqrt(),
			)
		};
		fill_rect(
			pixels,
			width,
			height,
			x + inset,
			y + row,
			w.saturating_sub(2 * inset),
			1,
			color,
		);
	}
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_kde_button(
	pixels: &mut [u32],
	width: usize,
	height: usize,
	x: usize,
	y: usize,
	w: usize,
	h: usize,
	scale: usize,
	background: u32,
	border: u32,
	accent: u32,
	hovered: bool,
) {
	let outline = if hovered {
		accent
	} else {
		blend(background, border, 90)
	};
	fill_round_rect(pixels, width, height, x, y, w, h, 7 * scale, outline);
	let face = if hovered {
		blend(background, accent, 52)
	} else {
		blend(background, border, 38)
	};
	fill_round_rect(
		pixels,
		width,
		height,
		x + scale,
		y + scale,
		w.saturating_sub(2 * scale),
		h.saturating_sub(2 * scale),
		6 * scale,
		face,
	);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(
	pixels: &mut [u32],
	width: usize,
	height: usize,
	x: usize,
	y: usize,
	text: &str,
	px: f32,
	ascent: i32,
	color: u32,
	font: &Font,
	glyphs: &mut GlyphCache,
) {
	let mut cursor_x = x as i32;
	for ch in text.chars() {
		let (metrics, bitmap) = glyphs
			.entry(glyph_cache_key(ch, px))
			.or_insert_with(|| font.rasterize(ch, px));
		let gx0 = cursor_x + metrics.xmin;
		let gy0 = y as i32 + ascent - metrics.ymin - metrics.height as i32;
		let draw_left = gx0.max(0);
		let draw_right = (gx0 + metrics.width as i32).min(width as i32);
		let draw_top = gy0.max(0);
		let draw_bottom = (gy0 + metrics.height as i32).min(height as i32);
		if draw_left < draw_right && draw_top < draw_bottom {
			let draw_width = (draw_right - draw_left) as usize;
			for py in draw_top..draw_bottom {
				let gy = (py - gy0) as usize;
				let src_start = gy * metrics.width + (draw_left - gx0) as usize;
				let dst_start = py as usize * width + draw_left as usize;
				blend_coverage_row(
					&mut pixels[dst_start..dst_start + draw_width],
					&bitmap[src_start..src_start + draw_width],
					color,
				);
			}
		}
		cursor_x += metrics.advance_width.ceil() as i32;
		if cursor_x >= width as i32 {
			break;
		}
	}
}
