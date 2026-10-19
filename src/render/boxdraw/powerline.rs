//! Powerline separators (U+E0B0–U+E0BF in Nerd Fonts' private use area):
//! triangles, chevrons, half circles and slants that span the whole cell.
use super::{Canvas, light_thickness, segment_distance};

pub(super) fn draw(canvas: &mut Canvas, code: u32) {
	let (w, h) = (canvas.w as f32, canvas.h as f32);
	let half = light_thickness(canvas.w) as f32 / 2.0;
	let mid = h / 2.0;
	// Mirrors x for the left-pointing shapes.
	let flip = code & 2 != 0;
	let across = move |x: f32| if flip { w - x } else { x };
	match code {
		0xe0b0 | 0xe0b2 => canvas.shape(|x, y| across(x) / w <= 1.0 - (y - mid).abs() / mid),
		0xe0b1 | 0xe0b3 => canvas.shape(|x, y| {
			let p = (across(x), y);
			segment_distance(p, (0.0, 0.0), (w, mid)).min(segment_distance(p, (w, mid), (0.0, h))) <= half
		}),
		0xe0b4 | 0xe0b6 => canvas.shape(|x, y| ellipse(across(x) / w, (y - mid) / mid) <= 1.0),
		0xe0b5 | 0xe0b7 => canvas.shape(|x, y| {
			let x = across(x);
			let inner = ellipse(
				x / (w - 2.0 * half).max(1.0),
				(y - mid) / (mid - 2.0 * half).max(1.0),
			);
			ellipse(x / w, (y - mid) / mid) <= 1.0 && inner >= 1.0
		}),
		0xe0b8 => canvas.shape(|x, y| y / h >= x / w),
		0xe0ba => canvas.shape(|x, y| y / h >= 1.0 - x / w),
		0xe0bc => canvas.shape(|x, y| y / h <= 1.0 - x / w),
		0xe0be => canvas.shape(|x, y| y / h <= x / w),
		0xe0b9 | 0xe0bf => canvas.shape(|x, y| segment_distance((x, y), (0.0, 0.0), (w, h)) <= half),
		_ => canvas.shape(|x, y| segment_distance((x, y), (0.0, h), (w, 0.0)) <= half),
	}
}

fn ellipse(x: f32, y: f32) -> f32 {
	x * x + y * y
}
