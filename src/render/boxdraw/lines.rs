//! Box drawing (U+2500–U+257F): light, heavy and double lines with exact
//! joints, dashes, rounded corners and diagonals.
use super::{Canvas, light_thickness, segment_distance};

const NONE: u8 = 0;
const LIGHT: u8 = 1;
const HEAVY: u8 = 2;
const DOUBLE: u8 = 3;
/// Characters drawn by their own code rather than from arms.
const SPECIAL: u8 = 0;

/// Packs the style of the left, right, up and down arms.
const fn arms(left: u8, right: u8, up: u8, down: u8) -> u8 {
	left | right << 2 | up << 4 | down << 6
}

/// The arms of each character from U+2500.
#[rustfmt::skip]
static ARMS: [u8; 128] = [
	// ─ ━ │ ┃
	arms(1, 1, 0, 0), arms(2, 2, 0, 0), arms(0, 0, 1, 1), arms(0, 0, 2, 2),
	// ┄ ┅ ┆ ┇
	SPECIAL, SPECIAL, SPECIAL, SPECIAL,
	// ┈ ┉ ┊ ┋
	SPECIAL, SPECIAL, SPECIAL, SPECIAL,
	// ┌ ┍ ┎ ┏
	arms(0, 1, 0, 1), arms(0, 2, 0, 1), arms(0, 1, 0, 2), arms(0, 2, 0, 2),
	// ┐ ┑ ┒ ┓
	arms(1, 0, 0, 1), arms(2, 0, 0, 1), arms(1, 0, 0, 2), arms(2, 0, 0, 2),
	// └ ┕ ┖ ┗
	arms(0, 1, 1, 0), arms(0, 2, 1, 0), arms(0, 1, 2, 0), arms(0, 2, 2, 0),
	// ┘ ┙ ┚ ┛
	arms(1, 0, 1, 0), arms(2, 0, 1, 0), arms(1, 0, 2, 0), arms(2, 0, 2, 0),
	// ├ ┝ ┞ ┟
	arms(0, 1, 1, 1), arms(0, 2, 1, 1), arms(0, 1, 2, 1), arms(0, 1, 1, 2),
	// ┠ ┡ ┢ ┣
	arms(0, 1, 2, 2), arms(0, 2, 2, 1), arms(0, 2, 1, 2), arms(0, 2, 2, 2),
	// ┤ ┥ ┦ ┧
	arms(1, 0, 1, 1), arms(2, 0, 1, 1), arms(1, 0, 2, 1), arms(1, 0, 1, 2),
	// ┨ ┩ ┪ ┫
	arms(1, 0, 2, 2), arms(2, 0, 2, 1), arms(2, 0, 1, 2), arms(2, 0, 2, 2),
	// ┬ ┭ ┮ ┯
	arms(1, 1, 0, 1), arms(2, 1, 0, 1), arms(1, 2, 0, 1), arms(2, 2, 0, 1),
	// ┰ ┱ ┲ ┳
	arms(1, 1, 0, 2), arms(2, 1, 0, 2), arms(1, 2, 0, 2), arms(2, 2, 0, 2),
	// ┴ ┵ ┶ ┷
	arms(1, 1, 1, 0), arms(2, 1, 1, 0), arms(1, 2, 1, 0), arms(2, 2, 1, 0),
	// ┸ ┹ ┺ ┻
	arms(1, 1, 2, 0), arms(2, 1, 2, 0), arms(1, 2, 2, 0), arms(2, 2, 2, 0),
	// ┼ ┽ ┾ ┿
	arms(1, 1, 1, 1), arms(2, 1, 1, 1), arms(1, 2, 1, 1), arms(2, 2, 1, 1),
	// ╀ ╁ ╂ ╃
	arms(1, 1, 2, 1), arms(1, 1, 1, 2), arms(1, 1, 2, 2), arms(2, 1, 2, 1),
	// ╄ ╅ ╆ ╇
	arms(1, 2, 2, 1), arms(2, 1, 1, 2), arms(1, 2, 1, 2), arms(2, 2, 2, 1),
	// ╈ ╉ ╊ ╋
	arms(2, 2, 1, 2), arms(2, 1, 2, 2), arms(1, 2, 2, 2), arms(2, 2, 2, 2),
	// ╌ ╍ ╎ ╏
	SPECIAL, SPECIAL, SPECIAL, SPECIAL,
	// ═ ║ ╒ ╓
	arms(3, 3, 0, 0), arms(0, 0, 3, 3), arms(0, 3, 0, 1), arms(0, 1, 0, 3),
	// ╔ ╕ ╖ ╗
	arms(0, 3, 0, 3), arms(3, 0, 0, 1), arms(1, 0, 0, 3), arms(3, 0, 0, 3),
	// ╘ ╙ ╚ ╛
	arms(0, 3, 1, 0), arms(0, 1, 3, 0), arms(0, 3, 3, 0), arms(3, 0, 1, 0),
	// ╜ ╝ ╞ ╟
	arms(1, 0, 3, 0), arms(3, 0, 3, 0), arms(0, 3, 1, 1), arms(0, 1, 3, 3),
	// ╠ ╡ ╢ ╣
	arms(0, 3, 3, 3), arms(3, 0, 1, 1), arms(1, 0, 3, 3), arms(3, 0, 3, 3),
	// ╤ ╥ ╦ ╧
	arms(3, 3, 0, 1), arms(1, 1, 0, 3), arms(3, 3, 0, 3), arms(3, 3, 1, 0),
	// ╨ ╩ ╪ ╫
	arms(1, 1, 3, 0), arms(3, 3, 3, 0), arms(3, 3, 1, 1), arms(1, 1, 3, 3),
	// ╬ ╭ ╮ ╯
	arms(3, 3, 3, 3), SPECIAL, SPECIAL, SPECIAL,
	// ╰ ╱ ╲ ╳
	SPECIAL, SPECIAL, SPECIAL, SPECIAL,
	// ╴ ╵ ╶ ╷
	arms(1, 0, 0, 0), arms(0, 0, 1, 0), arms(0, 1, 0, 0), arms(0, 0, 0, 1),
	// ╸ ╹ ╺ ╻
	arms(2, 0, 0, 0), arms(0, 0, 2, 0), arms(0, 2, 0, 0), arms(0, 0, 0, 2),
	// ╼ ╽ ╾ ╿
	arms(1, 2, 0, 0), arms(0, 0, 1, 2), arms(2, 1, 0, 0), arms(0, 0, 2, 1),
];

/// Positions across one axis of the cell: where light, heavy and double
/// strokes lie, centered the same way in every cell so they line up.
#[derive(Clone, Copy)]
struct Axis {
	length: usize,
	light: usize,
}

impl Axis {
	/// Where a light stroke starts; double strokes flank it.
	fn center(self) -> usize {
		self.length.saturating_sub(self.light) / 2
	}

	/// The strokes of a line of `style`, as pixel ranges; the second is
	/// empty unless the line is double.
	fn strokes(self, style: u8) -> [(usize, usize); 2] {
		let (c, t) = (self.center(), self.light);
		match style {
			HEAVY => {
				let start = c.saturating_sub(t / 2);
				[(start, start + 2 * t), (0, 0)]
			}
			DOUBLE => [(c.saturating_sub(t), c), (c + t, c + 2 * t)],
			_ => [(c, c + t), (0, 0)],
		}
	}

	/// Where an arm running along the other axis meets the lines `a` and
	/// `b` running along this one: the start for an arm heading to the far
	/// end, the end otherwise. Against a double line, `near` stops at its
	/// closer stroke, otherwise the arm runs on to the farther one.
	fn edge(self, (a, b): (u8, u8), to_end: bool, near: bool) -> usize {
		let (c, t) = (self.center(), self.light);
		if a == DOUBLE || b == DOUBLE {
			return match (to_end, near) {
				(true, true) => c + t,
				(true, false) => c.saturating_sub(t),
				(false, true) => c,
				(false, false) => c + 2 * t,
			};
		}
		let mut range = (usize::MAX, 0);
		for style in [a, b].into_iter().filter(|&style| style != NONE) {
			let (start, end) = self.strokes(style)[0];
			range = (range.0.min(start), range.1.max(end));
		}
		if range.0 == usize::MAX {
			range = self.strokes(LIGHT)[0];
		}
		if to_end { range.0 } else { range.1 }
	}
}

pub(super) fn draw(canvas: &mut Canvas, code: u32) {
	let t = light_thickness(canvas.w);
	let x = Axis {
		length: canvas.w,
		light: t,
	};
	let y = Axis {
		length: canvas.h,
		light: t,
	};
	match code {
		0x2504..=0x250b => {
			let (style, dashes) = (
				LIGHT + (code as u8 & 1),
				[3, 3, 4, 4][(code as usize - 0x2504) / 2],
			);
			dashed(canvas, (x, y), style, dashes, (code - 0x2504) & 2 != 0);
		}
		0x254c..=0x254f => dashed(canvas, (x, y), LIGHT + (code as u8 & 1), 2, code >= 0x254e),
		0x256d..=0x2570 => {
			let (right, down) =
				[(true, true), (false, true), (false, false), (true, false)][code as usize - 0x256d];
			arc(canvas, (x, y), right, down);
		}
		0x2571..=0x2573 => diagonals(canvas, t, code != 0x2572, code != 0x2571),
		_ => joints(canvas, (x, y), ARMS[code as usize - 0x2500]),
	}
}

/// Draws the four arms of a line character, joined at the center.
fn joints(canvas: &mut Canvas, (x, y): (Axis, Axis), packed: u8) {
	let [left, right, up, down] = [0, 2, 4, 6].map(|shift| packed >> shift & 3);
	let (w, h) = (canvas.w, canvas.h);
	// Horizontal arms, against the vertical line made of the up and down arms.
	for (style, opposite, to_end) in [(left, right, false), (right, left, true)] {
		for (k, stroke) in y.strokes(style).into_iter().enumerate() {
			if style == NONE || stroke.0 >= stroke.1 {
				continue;
			}
			let near = near(style, k, (up, down), opposite);
			let edge = x.edge((up, down), to_end, near);
			let span = if to_end { (edge, w) } else { (0, edge) };
			canvas.fill(span, stroke, 255);
		}
	}
	for (style, opposite, to_end) in [(up, down, false), (down, up, true)] {
		for (k, stroke) in x.strokes(style).into_iter().enumerate() {
			if style == NONE || stroke.0 >= stroke.1 {
				continue;
			}
			let near = near(style, k, (left, right), opposite);
			let edge = y.edge((left, right), to_end, near);
			let span = if to_end { (edge, h) } else { (0, edge) };
			canvas.fill(stroke, span, 255);
		}
	}
}

/// Whether stroke `k` of an arm stops at the closer stroke of a double
/// line crossing it. A double arm's outer stroke turns the corner (runs to
/// the farther stroke) unless an arm continues on its side; a single arm
/// crosses unless it is a tee off a straight double line.
fn near(style: u8, k: usize, (before, after): (u8, u8), opposite: u8) -> bool {
	if style == DOUBLE {
		[before, after][k] != NONE
	} else {
		before != NONE && after != NONE && opposite == NONE
	}
}

/// A line of `dashes` evenly spaced dashes, each centered in its share of
/// the cell so the pattern continues across cells.
fn dashed(canvas: &mut Canvas, (x, y): (Axis, Axis), style: u8, dashes: usize, vertical: bool) {
	let (along, across) = if vertical {
		(canvas.h, x)
	} else {
		(canvas.w, y)
	};
	let stroke = across.strokes(style)[0];
	for i in 0..dashes {
		let (start, end) = (i * along / dashes, (i + 1) * along / dashes);
		let gap = ((end - start) / 3).max(1);
		let dash = (start + gap / 2, end - (gap - gap / 2));
		if vertical {
			canvas.fill(stroke, dash, 255);
		} else {
			canvas.fill(dash, stroke, 255);
		}
	}
}

/// A rounded corner joining the center of the cell's right or left edge
/// to the center of its bottom or top edge.
fn arc(canvas: &mut Canvas, (x, y): (Axis, Axis), right: bool, down: bool) {
	let half = x.light as f32 / 2.0;
	let (cx, cy) = (x.center() as f32 + half, y.center() as f32 + half);
	let (w, h) = (canvas.w as f32, canvas.h as f32);
	let (dx, dy) = (
		if right { 1.0 } else { -1.0 },
		if down { 1.0 } else { -1.0 },
	);
	let reach =
		|center: f32, length: f32, forward: bool| if forward { length - center } else { center };
	// The last pixel toward each edge stays straight, so it matches the
	// straight lines it meets exactly.
	let r = (reach(cx, w, right).min(reach(cy, h, down)) - 1.0).max(half);
	let (ax, ay) = (cx + dx * r, cy + dy * r);
	canvas.shape(|px, py| {
		let (along_x, along_y) = ((px - ax) * dx, (py - ay) * dy);
		if along_x >= 0.0 {
			(py - cy).abs() <= half
		} else if along_y >= 0.0 {
			(px - cx).abs() <= half
		} else {
			let distance = ((px - ax) * (px - ax) + (py - ay) * (py - ay)).sqrt();
			(distance - r).abs() <= half
		}
	});
}

/// Diagonals from corner to corner, so they continue into the next cell.
fn diagonals(canvas: &mut Canvas, t: usize, rising: bool, falling: bool) {
	let (w, h) = (canvas.w as f32, canvas.h as f32);
	let half = t as f32 / 2.0;
	canvas.shape(|px, py| {
		(rising && segment_distance((px, py), (w, 0.0), (0.0, h)) <= half)
			|| (falling && segment_distance((px, py), (0.0, 0.0), (w, h)) <= half)
	});
}
