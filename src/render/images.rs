//! Draws image placements: source rectangle, nearest-neighbor scaling,
//! alpha blending and clipping.
use super::blend::blend;
use crate::graphics::{Placement, Rect};

/// Draws `placement` with its first cell's top-left corner at `at` into
/// `pixels` (rows of `stride` pixels), touching only pixels inside `clip`.
pub(crate) fn draw_placement(
	pixels: &mut [u32],
	stride: usize,
	clip: Rect,
	placement: &Placement,
	at: (isize, isize),
	cell: (usize, usize),
) {
	let (width, height) = placement.size(cell);
	let source = placement.source;
	if width == 0 || height == 0 || source.width == 0 || source.height == 0 {
		return;
	}
	let left = at.0 + placement.offset.0 as isize;
	let top = at.1 + placement.offset.1 as isize;
	// The visible part of the destination rectangle.
	let x0 = left.max(clip.x as isize);
	let y0 = top.max(clip.y as isize);
	let x1 = (left + width as isize).min((clip.x + clip.width) as isize);
	let y1 = (top + height as isize).min((clip.y + clip.height) as isize);
	if x0 >= x1 || y0 >= y1 {
		return;
	}
	let bitmap = &placement.bitmap;
	// Source column for each visible destination column.
	let columns: Vec<usize> = (x0..x1)
		.map(|x| source.x + (x - left) as usize * source.width / width)
		.collect();
	let unscaled = width == source.width;
	for y in y0..y1 {
		let source_y = source.y + (y - top) as usize * source.height / height;
		let source_row = &bitmap.pixels[source_y * bitmap.width..][..bitmap.width];
		let start = y as usize * stride + x0 as usize;
		let target = &mut pixels[start..start + columns.len()];
		if bitmap.opaque && unscaled {
			let from = columns[0];
			for (out, &pixel) in target
				.iter_mut()
				.zip(&source_row[from..from + columns.len()])
			{
				*out = pixel & 0xff_ffff;
			}
			continue;
		}
		for (out, &column) in target.iter_mut().zip(&columns) {
			let pixel = source_row[column];
			*out = blend(*out, pixel & 0xff_ffff, pixel >> 24);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics::{Bitmap, Extent};
	use std::sync::Arc;

	const RED: u32 = 0xffff0000;
	const BLUE: u32 = 0xff0000ff;

	fn placement(width: usize, height: usize, pixels: Vec<u32>) -> Placement {
		Placement::natural(Arc::new(Bitmap::new(width, height, pixels)), 0, 0)
	}

	fn canvas() -> Vec<u32> {
		vec![0x101010; 6 * 4]
	}

	fn everything() -> Rect {
		Rect {
			x: 0,
			y: 0,
			width: 6,
			height: 4,
		}
	}

	#[test]
	fn opaque_pixels_are_copied_without_alpha_and_clipped() {
		let mut pixels = canvas();
		let image = placement(2, 2, vec![RED, BLUE, BLUE, RED]);
		draw_placement(&mut pixels, 6, everything(), &image, (5, 3), (1, 1));
		assert_eq!(pixels[3 * 6 + 5], 0xff0000);
		assert_eq!(pixels.iter().filter(|&&p| p != 0x101010).count(), 1);
		// Fully outside: nothing happens, nothing panics.
		draw_placement(&mut pixels, 6, everything(), &image, (-5, 9), (1, 1));
		draw_placement(&mut pixels, 6, everything(), &image, (-1, -1), (1, 1));
		assert_eq!(pixels[0], 0xff0000);
	}

	#[test]
	fn alpha_is_blended_and_transparent_pixels_are_skipped() {
		let mut pixels = vec![0x000000; 2];
		let image = placement(2, 1, vec![0x80ffffff, 0x00ff0000]);
		let clip = Rect {
			x: 0,
			y: 0,
			width: 2,
			height: 1,
		};
		draw_placement(&mut pixels, 2, clip, &image, (0, 0), (1, 1));
		assert_eq!(pixels, [0x808080, 0x000000]);
	}

	#[test]
	fn cell_extents_scale_the_source_rectangle() {
		let mut pixels = canvas();
		let mut image = placement(2, 2, vec![RED, BLUE, BLUE, RED]);
		image.source = Rect {
			x: 1,
			y: 0,
			width: 1,
			height: 2,
		};
		image.extent = Extent::Cells { cols: 1, rows: 2 };
		image.offset = (1, 0);
		draw_placement(&mut pixels, 6, everything(), &image, (0, 0), (2, 2));
		let row = |y: usize| pixels[y * 6..y * 6 + 4].to_vec();
		assert_eq!(row(0), [0x101010, 0x0000ff, 0x0000ff, 0x101010]);
		assert_eq!(row(1), row(0));
		assert_eq!(row(2), [0x101010, 0xff0000, 0xff0000, 0x101010]);
	}
}
