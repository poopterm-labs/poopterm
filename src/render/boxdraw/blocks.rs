//! Block elements (U+2580–U+259F), Braille patterns (U+2800–U+28FF) and
//! sextants (U+1FB00–U+1FB3B), on boundaries shared by complementary pieces.
use super::Canvas;

/// The pixel boundary `k` eighths of the way along `length`.
fn eighths(length: usize, k: usize) -> usize {
	(length * k + 4) / 8
}

pub(super) fn block(canvas: &mut Canvas, code: u32) {
	let (w, h) = (canvas.w, canvas.h);
	let all = (0, usize::MAX);
	match code {
		0x2580 => canvas.fill(all, (0, eighths(h, 4)), 255),
		0x2581..=0x2588 => canvas.fill(all, (eighths(h, 8 - (code - 0x2580) as usize), h), 255),
		0x2589..=0x258f => canvas.fill((0, eighths(w, (0x2590 - code) as usize)), all, 255),
		0x2590 => canvas.fill((eighths(w, 4), w), all, 255),
		// Shades are even tints, which tile without the moiré of a pattern.
		0x2591..=0x2593 => canvas.fill(all, all, [64, 128, 191][(code - 0x2591) as usize]),
		0x2594 => canvas.fill(all, (0, eighths(h, 1)), 255),
		0x2595 => canvas.fill((eighths(w, 7), w), all, 255),
		_ => {
			// Quadrants as bits: upper left, upper right, lower left, lower right.
			let bits = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14][(code - 0x2596) as usize];
			let (mx, my) = (eighths(w, 4), eighths(h, 4));
			let quadrants = [
				((0, mx), (0, my)),
				((mx, w), (0, my)),
				((0, mx), (my, h)),
				((mx, w), (my, h)),
			];
			for (i, (xs, ys)) in quadrants.into_iter().enumerate() {
				if bits >> i & 1 != 0 {
					canvas.fill(xs, ys, 255);
				}
			}
		}
	}
}

/// Round dots in a 2×4 grid; bit `n` of the pattern raises dot `n + 1`.
pub(super) fn braille(canvas: &mut Canvas, pattern: u8) {
	// (column, row) of dots 1 to 8.
	const DOTS: [(f32, f32); 8] = [
		(0.0, 0.0),
		(0.0, 1.0),
		(0.0, 2.0),
		(1.0, 0.0),
		(1.0, 1.0),
		(1.0, 2.0),
		(0.0, 3.0),
		(1.0, 3.0),
	];
	let (slot_w, slot_h) = (canvas.w as f32 / 2.0, canvas.h as f32 / 4.0);
	let radius = slot_w.min(slot_h) * 0.25 + 0.25;
	let dots: Vec<(f32, f32)> = (0..8)
		.filter(|bit| pattern >> bit & 1 != 0)
		.map(|bit| {
			let (column, row) = DOTS[bit];
			((column + 0.5) * slot_w, (row + 0.5) * slot_h)
		})
		.collect();
	canvas.shape(|x, y| {
		dots
			.iter()
			.any(|&(cx, cy)| (x - cx) * (x - cx) + (y - cy) * (y - cy) <= radius * radius)
	});
}

/// Sextant `index` from U+1FB00: blocks of a 2×3 grid, as the bits of a
/// pattern that skips the two halves already in block elements.
pub(super) fn sextant(canvas: &mut Canvas, index: u32) {
	let mut pattern = index + 1;
	for skipped in [21, 42] {
		if pattern >= skipped {
			pattern += 1;
		}
	}
	let (w, h) = (canvas.w, canvas.h);
	let thirds = [0, (h + 1) / 3, (2 * h + 1) / 3, h];
	let columns = [(0, eighths(w, 4)), (eighths(w, 4), w)];
	for bit in 0..6 {
		if pattern >> bit & 1 != 0 {
			let row = bit as usize / 2;
			canvas.fill(
				columns[bit as usize % 2],
				(thirds[row], thirds[row + 1]),
				255,
			);
		}
	}
}
