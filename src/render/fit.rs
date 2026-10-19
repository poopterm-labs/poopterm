//! Fitting glyphs whose ink is wider or taller than their cells, such as
//! Nerd Font icons and arrows: they spill into a blank cell to their right,
//! and shrink where there is no room, instead of being cut off.

/// The factor to rasterize a glyph of `ink` (width, height) pixels by so
/// it fits `room` (width, height); 1 when it already fits. Rounding the
/// scaled box outward can add a pixel, which the margin absorbs.
pub(crate) fn fit_scale(ink: (usize, usize), room: (usize, usize)) -> f32 {
	let factor = |ink: usize, room: usize| {
		if ink > room {
			room.saturating_sub(1).max(1) as f32 / ink as f32
		} else {
			1.0
		}
	};
	factor(ink.0, room.0).min(factor(ink.1, room.1))
}

/// Where a bitmap `width` pixels wide starts, from the left of its room,
/// when it was drawn at offset `xmin`: moved inside the room if it fits.
pub(crate) fn fit_left(xmin: i32, width: usize, room: usize) -> i32 {
	xmin.clamp(0, (room as i32 - width as i32).max(0))
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
