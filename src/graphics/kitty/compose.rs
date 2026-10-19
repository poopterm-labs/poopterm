//! Composes frame pixels: copies or alpha-blends a rectangle of one bitmap
//! onto another (kitty `a=f` frame data and `a=c`).
use crate::graphics::{Bitmap, Rect};

/// Draws the `from` rectangle of `source` onto `target` at `to`, alpha
/// blending unless `overwrite`. Both rectangles must lie inside their
/// bitmaps.
pub(crate) fn compose(
	target: &mut Bitmap,
	to: (usize, usize),
	source: &Bitmap,
	from: Rect,
	overwrite: bool,
) {
	for row in 0..from.height {
		let source_row = &source.pixels[(from.y + row) * source.width + from.x..][..from.width];
		let start = (to.1 + row) * target.width + to.0;
		let target_row = &mut target.pixels[start..start + from.width];
		if overwrite {
			target_row.copy_from_slice(source_row);
		} else {
			for (out, &pixel) in target_row.iter_mut().zip(source_row) {
				*out = over(*out, pixel);
			}
		}
	}
	target.opaque = target.pixels.iter().all(|&pixel| pixel >> 24 == 0xff);
}

/// `source` over `target`, both `0xAARRGGBB` with straight alpha.
fn over(target: u32, source: u32) -> u32 {
	let source_alpha = source >> 24;
	match source_alpha {
		0xff => return source,
		0 => return target,
		_ => {}
	}
	// Weights scaled by 255 * 255.
	let source_weight = source_alpha * 255;
	let target_weight = (target >> 24) * (255 - source_alpha);
	let total = source_weight + target_weight;
	let channel = |shift: u32| {
		let mix =
			((source >> shift) & 0xff) * source_weight + ((target >> shift) & 0xff) * target_weight;
		(mix + total / 2) / total
	};
	((total + 127) / 255) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bitmap(width: usize, height: usize, pixel: u32) -> Bitmap {
		Bitmap::new(width, height, vec![pixel; width * height])
	}

	#[test]
	fn overwriting_copies_and_blending_mixes_straight_alpha() {
		let mut target = bitmap(3, 2, 0xff_0000ff);
		let source = Bitmap::new(2, 1, vec![0x80ff_0000, 0x0000_ff00]);
		let whole = Rect {
			x: 0,
			y: 0,
			width: 2,
			height: 1,
		};
		compose(&mut target, (1, 1), &source, whole, false);
		assert_eq!(target.pixels[..4], [0xff_0000ff; 4]);
		// Half red over opaque blue; a transparent pixel changes nothing.
		assert_eq!(target.pixels[4..], [0xff80_007f, 0xff_0000ff]);
		assert!(target.opaque);
		compose(&mut target, (1, 1), &source, whole, true);
		assert_eq!(target.pixels[4..], [0x80ff_0000, 0x0000_ff00]);
		assert!(!target.opaque);
		// Blending onto transparent pixels keeps the source color.
		let mut clear = bitmap(1, 1, 0);
		compose(
			&mut clear,
			(0, 0),
			&source,
			Rect { width: 1, ..whole },
			false,
		);
		assert_eq!(clear.pixels, [0x80ff_0000]);
	}
}
