//! TrueType outlines (`glyf` and `loca`): simple glyphs of quadratic
//! contours, and composite glyphs built from transformed components.
use super::{
	face::{Bounds, Face, Sink},
	read::{Bytes, f2dot14},
};

/// Composite glyphs nest rarely more than two levels; this bounds hostile
/// fonts that reference themselves.
const MAX_DEPTH: usize = 8;
/// More points than any real glyph has.
const MAX_POINTS: usize = 1 << 15;

/// An affine transform `[a b c d e f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy)]
struct Transform([f32; 6]);

impl Transform {
	const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

	fn apply(&self, x: f32, y: f32) -> (f32, f32) {
		let [a, b, c, d, e, f] = self.0;
		(a * x + c * y + e, b * x + d * y + f)
	}

	/// `inner` applied first, then `self`.
	fn then(&self, inner: &Self) -> Self {
		let [a, b, c, d, e, f] = self.0;
		let [a2, b2, c2, d2, e2, f2] = inner.0;
		Self([
			a * a2 + c * b2,
			b * a2 + d * b2,
			a * c2 + c * d2,
			b * c2 + d * d2,
			a * e2 + c * f2 + e,
			b * e2 + d * f2 + f,
		])
	}
}

/// Tracks the bounds of everything sent through it.
struct Bounded<'a> {
	sink: &'a mut dyn Sink,
	bounds: Option<Bounds>,
	transform: Transform,
}

impl Bounded<'_> {
	fn point(&mut self, x: f32, y: f32) -> (f32, f32) {
		let (x, y) = self.transform.apply(x, y);
		Bounds::include(&mut self.bounds, x, y);
		(x, y)
	}
}

pub(crate) fn outline(face: &Face, glyph: u16, sink: &mut dyn Sink) -> Option<Bounds> {
	let mut bounded = Bounded {
		sink,
		bounds: None,
		transform: Transform::IDENTITY,
	};
	draw(face, glyph, &mut bounded, 0)?;
	bounded.bounds
}

/// The bytes of `glyph` in `glyf`; empty for glyphs without an outline.
fn glyph_data(face: &Face, glyph: u16) -> Option<&[u8]> {
	let (data, index) = (&*face.data, usize::from(glyph));
	let loca = face.loca.bytes(data);
	let (start, end) = if face.long_loca {
		(
			loca.u32_at(4 * index)? as usize,
			loca.u32_at(4 * index + 4)? as usize,
		)
	} else {
		(
			2 * usize::from(loca.u16_at(2 * index)?),
			2 * usize::from(loca.u16_at(2 * index + 2)?),
		)
	};
	face.glyf.bytes(data).get(start..end)
}

fn draw(face: &Face, glyph: u16, out: &mut Bounded, depth: usize) -> Option<()> {
	let data = glyph_data(face, glyph)?;
	if data.is_empty() {
		return Some(());
	}
	match data.i16_at(0)? {
		contours @ 0.. => simple(data, contours as usize, out),
		_ if depth < MAX_DEPTH => composite(face, data, out, depth),
		_ => None,
	}
}

fn simple(data: &[u8], contours: usize, out: &mut Bounded) -> Option<()> {
	let ends: Vec<usize> = (0..contours)
		.map(|i| data.u16_at(10 + 2 * i).map(usize::from))
		.collect::<Option<_>>()?;
	let points = ends.last().map_or(0, |&last| last + 1);
	if points > MAX_POINTS {
		return None;
	}
	let instructions = usize::from(data.u16_at(10 + 2 * contours)?);
	let mut at = 12 + 2 * contours + instructions;
	// Flags, with runs: bit 3 repeats a flag the next byte's times.
	let mut flags = Vec::with_capacity(points);
	while flags.len() < points {
		let flag = data.u8_at(at)?;
		at += 1;
		let repeat = if flag & 8 != 0 {
			at += 1;
			usize::from(data.u8_at(at - 1)?)
		} else {
			0
		};
		for _ in 0..=repeat.min(points - flags.len() - 1) {
			flags.push(flag);
		}
	}
	// Coordinates are deltas: a short (1-byte) value with a sign bit, or the
	// same as before, or a 2-byte value. Both axes decode into one buffer.
	let mut all: Vec<(f32, f32, bool)> = flags
		.iter()
		.map(|&flag| (0.0, 0.0, flag & 1 != 0))
		.collect();
	let mut read_axis = |short: u8, same: u8, set: fn(&mut (f32, f32, bool), f32)| -> Option<()> {
		let mut value = 0i32;
		for (&flag, point) in flags.iter().zip(&mut all) {
			if flag & short != 0 {
				let delta = i32::from(data.u8_at(at)?);
				at += 1;
				value += if flag & same != 0 { delta } else { -delta };
			} else if flag & same == 0 {
				value += i32::from(data.i16_at(at)?);
				at += 2;
			}
			set(point, value as f32);
		}
		Some(())
	};
	read_axis(2, 16, |point, x| point.0 = x)?;
	read_axis(4, 32, |point, y| point.1 = y)?;
	let mut start = 0;
	for &end in &ends {
		if end < start || end >= points {
			return None;
		}
		contour(&all[start..=end], out);
		start = end + 1;
	}
	Some(())
}

/// Sends one closed quadratic contour. Two off-curve points in a row imply
/// an on-curve point midway; a contour may start off the curve.
fn contour(all: &[(f32, f32, bool)], out: &mut Bounded) {
	let Some(&(fx, fy, first_on)) = all.first() else {
		return;
	};
	let (sx, sy) = if first_on {
		(fx, fy)
	} else {
		match all.last() {
			Some(&(lx, ly, true)) => (lx, ly),
			Some(&(lx, ly, false)) => ((lx + fx) / 2.0, (ly + fy) / 2.0),
			None => return,
		}
	};
	let (x, y) = out.point(sx, sy);
	out.sink.move_to(x, y);
	let mut control: Option<(f32, f32)> = None;
	let rest = all.iter().skip(usize::from(first_on));
	for &(x, y, on) in rest.chain(std::iter::once(&(sx, sy, true))) {
		match (on, control) {
			(true, None) => {
				let (x, y) = out.point(x, y);
				out.sink.line_to(x, y);
			}
			(true, Some((cx, cy))) => {
				let ((cx, cy), (x, y)) = (out.point(cx, cy), out.point(x, y));
				out.sink.quad_to(cx, cy, x, y);
				control = None;
			}
			(false, None) => control = Some((x, y)),
			(false, Some((cx, cy))) => {
				let (mx, my) = ((cx + x) / 2.0, (cy + y) / 2.0);
				let ((cx, cy), (mx, my)) = (out.point(cx, cy), out.point(mx, my));
				out.sink.quad_to(cx, cy, mx, my);
				control = Some((x, y));
			}
		}
	}
	out.sink.close();
}

fn composite(face: &Face, data: &[u8], out: &mut Bounded, depth: usize) -> Option<()> {
	const WORDS: u16 = 1;
	const XY_VALUES: u16 = 2;
	const SCALE: u16 = 8;
	const MORE: u16 = 0x20;
	const XY_SCALE: u16 = 0x40;
	const TWO_BY_TWO: u16 = 0x80;
	const SCALED_OFFSET: u16 = 0x800;
	let mut at = 10;
	let parent = out.transform;
	loop {
		let (flags, glyph) = (data.u16_at(at)?, data.u16_at(at + 2)?);
		at += 4;
		let (dx, dy) = if flags & WORDS != 0 {
			at += 4;
			(data.i16_at(at - 4)? as f32, data.i16_at(at - 2)? as f32)
		} else {
			at += 2;
			(
				data.u8_at(at - 2)? as i8 as f32,
				data.u8_at(at - 1)? as i8 as f32,
			)
		};
		// Point-matched components (no XY values) are placed unshifted.
		let (dx, dy) = if flags & XY_VALUES != 0 {
			(dx, dy)
		} else {
			(0.0, 0.0)
		};
		let [a, b, c, d] = if flags & SCALE != 0 {
			at += 2;
			let s = f2dot14(data.u16_at(at - 2)?);
			[s, 0.0, 0.0, s]
		} else if flags & XY_SCALE != 0 {
			at += 4;
			[
				f2dot14(data.u16_at(at - 4)?),
				0.0,
				0.0,
				f2dot14(data.u16_at(at - 2)?),
			]
		} else if flags & TWO_BY_TWO != 0 {
			at += 8;
			[
				f2dot14(data.u16_at(at - 8)?),
				f2dot14(data.u16_at(at - 6)?),
				f2dot14(data.u16_at(at - 4)?),
				f2dot14(data.u16_at(at - 2)?),
			]
		} else {
			[1.0, 0.0, 0.0, 1.0]
		};
		let (e, f) = if flags & SCALED_OFFSET != 0 {
			(a * dx + c * dy, b * dx + d * dy)
		} else {
			(dx, dy)
		};
		out.transform = parent.then(&Transform([a, b, c, d, e, f]));
		draw(face, glyph, out, depth + 1)?;
		out.transform = parent;
		if flags & MORE == 0 {
			return Some(());
		}
	}
}
