//! Differential tests of the font engine against fontdue, the crate it
//! replaces: metrics must agree exactly and coverage within rounding.
use super::font::Font;

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");
const SIZES: [f32; 7] = [8.0, 11.0, 12.5, 14.0, 17.0, 24.0, 48.0];

fn sample() -> impl Iterator<Item = char> {
	(' '..='~').chain('¡'..='ÿ').chain([
		'─', '│', '┼', '█', '▀', '░', '→', '€', 'ﬁ', 'Ω', 'ж', 'λ', '\u{fffd}',
	])
}

/// The coverage at pixel `(x, y)` (y up from the baseline) of a bitmap.
fn pixel(metrics: (i32, i32, usize, usize), coverage: &[u8], x: i32, y: i32) -> u8 {
	let (xmin, ymin, width, height) = metrics;
	let (column, row) = (x - xmin, ymin + height as i32 - 1 - y);
	if column < 0 || row < 0 || column as usize >= width || row as usize >= height {
		return 0;
	}
	coverage[row as usize * width + column as usize]
}

/// Compares every sample glyph; returns how many were checked and the
/// disagreements. Boxes may differ by a pixel: fontdue's rounding adds an
/// empty column when an edge falls exactly on a pixel boundary, and its
/// quadratic stand-ins for cubic (CFF) curves can move an edge.
fn compare(bytes: &[u8], index: u32, name: &str) -> (usize, Vec<String>) {
	let ours =
		Font::from_bytes(bytes.to_vec(), index).unwrap_or_else(|error| panic!("{name}: {error}"));
	let exact_boxes = !ours.is_cff();
	let settings = fontdue::FontSettings {
		collection_index: index,
		..Default::default()
	};
	let theirs = fontdue::Font::from_bytes(bytes, settings).unwrap();
	let (mut checked, mut failures) = (0, Vec::new());
	for px in SIZES {
		let (a, b) = (
			ours.horizontal_line_metrics(px).unwrap(),
			theirs.horizontal_line_metrics(px).unwrap(),
		);
		assert!((a.ascent - b.ascent).abs() < 1e-3, "{name} {px}px ascent");
		assert!(
			(a.descent - b.descent).abs() < 1e-3,
			"{name} {px}px descent"
		);
		assert!(
			(a.new_line_size - b.new_line_size).abs() < 1e-3,
			"{name} {px}px line size"
		);
		for ch in sample() {
			let glyphs = (ours.lookup_glyph_index(ch), theirs.lookup_glyph_index(ch));
			// fontdue also reads the Mac Roman subtable as if its byte
			// codes were Unicode, inventing glyphs for U+0080..=U+00FF.
			if glyphs.0 == 0 && ('\u{80}'..='\u{ff}').contains(&ch) {
				continue;
			}
			if glyphs.0 != glyphs.1 {
				failures.push(format!(
					"{name} {ch:?}: glyph {} but fontdue {}",
					glyphs.0, glyphs.1
				));
				continue;
			}
			let (m, coverage) = ours.rasterize(ch, px);
			let (n, expected) = theirs.rasterize(ch, px);
			let context = format!("{name} {ch:?} at {px}px: ours {m:?}, fontdue {n:?}");
			assert!(
				(m.advance_width - n.advance_width).abs() < 1e-3,
				"{context}"
			);
			if n.width == 0 || n.height == 0 {
				continue;
			}
			let (ours_box, theirs_box) = (
				(m.xmin, m.ymin, m.width, m.height),
				(n.xmin, n.ymin, n.width, n.height),
			);
			checked += 1;
			let edges = |(x, y, w, h): (i32, i32, usize, usize)| [x, y, x + w as i32, y + h as i32];
			let apart = edges(ours_box)
				.iter()
				.zip(edges(theirs_box))
				.map(|(a, b)| a.abs_diff(b))
				.max();
			if apart > Some(1) {
				failures.push(format!("{context}: boxes differ"));
				continue;
			}
			let (left, bottom) = (m.xmin.min(n.xmin), m.ymin.min(n.ymin));
			let (right, top) = (
				(m.xmin + m.width as i32).max(n.xmin + n.width as i32),
				(m.ymin + m.height as i32).max(n.ymin + n.height as i32),
			);
			let (mut worst, mut total, mut count) = (0, 0usize, 0usize);
			for y in bottom..top {
				for x in left..right {
					let difference =
						pixel(ours_box, &coverage, x, y).abs_diff(pixel(theirs_box, &expected, x, y));
					(worst, total, count) = (
						worst.max(difference),
						total + usize::from(difference),
						count + 1,
					);
				}
			}
			// fontdue flattens curves into coarser chords, losing up to
			// about 56/255 of a pixel along round edges, where we stay
			// within 0.02 px of the curve. Its quadratic stand-ins for
			// cubics stray further, so CFF fonts are held to the mean only.
			let close = if exact_boxes {
				worst <= 56
			} else {
				total <= 12 * count
			};
			if !close {
				failures.push(format!(
					"{context}: worst pixel {worst}, mean {}",
					total / count
				));
			}
		}
	}
	(checked, failures)
}

#[test]
fn bundled_font_matches_fontdue() {
	let (checked, failures) = compare(BUNDLED, 0, "bundled");
	assert!(checked > 1000);
	assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn metrics_without_rasterizing_match_the_bitmap() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	for ch in ['M', 'g', ' ', '█'] {
		assert_eq!(font.metrics(ch, 15.0), font.rasterize(ch, 15.0).0, "{ch:?}");
	}
}

#[test]
fn points_that_draw_nothing_leave_the_bounds_alone() {
	use super::{
		face::{Bounds, Sink},
		raster::Path,
	};
	let mut path = Path::default();
	// One-point contours, as fonts use for anchors and bounding-box marks.
	path.move_to(-500.0, -500.0);
	path.line_to(-500.0, -500.0);
	path.close();
	path.move_to(900.0, 900.0);
	path.close();
	path.move_to(0.0, 0.0);
	path.line_to(100.0, 0.0);
	path.quad_to(150.0, 50.0, 100.0, 100.0);
	path.line_to(0.0, 100.0);
	path.close();
	let expected = Bounds {
		x_min: 0.0,
		y_min: 0.0,
		x_max: 125.0,
		y_max: 100.0,
	};
	assert_eq!(path.bounds(), Some(expected));
}

#[test]
fn malformed_fonts_are_rejected_not_panicked_on() {
	for len in [0, 4, 12, 100, 1000] {
		assert!(
			Font::from_bytes(&BUNDLED[..len], 0).is_err(),
			"truncated to {len}"
		);
	}
	let mut bytes = BUNDLED.to_vec();
	// Scramble the table data but keep the directory.
	for (i, byte) in bytes.iter_mut().enumerate().skip(400) {
		*byte = byte.wrapping_mul(31).wrapping_add(i as u8);
	}
	if let Ok(font) = Font::from_bytes(bytes, 0) {
		for ch in sample() {
			font.rasterize(ch, 16.0);
		}
	}
}

/// Every installed font: `cargo test --release -- --ignored installed_fonts`.
#[test]
#[ignore = "reads the fonts installed on this machine"]
fn installed_fonts_match_fontdue() {
	let cache = super::system_fonts();
	let (mut checked, mut failures) = (0, Vec::new());
	for ((path, index), face) in cache.faces() {
		let Ok(bytes) = std::fs::read(path) else {
			continue;
		};
		// fontdue reads glyf and CFF fonts; skip what neither of us supports.
		if fontdue::Font::from_bytes(
			bytes.as_slice(),
			fontdue::FontSettings {
				collection_index: index,
				..Default::default()
			},
		)
		.is_err()
		{
			continue;
		}
		// fontdue never closes CFF2 contours, which have no endchar, and
		// fills those glyphs solid down to the bottom of the box.
		if Font::from_bytes(bytes.clone(), index).is_ok_and(|font| font.is_cff2()) {
			continue;
		}
		let name = format!("{} ({})", face.family, path.display());
		let (count, mut found) = compare(&bytes, index, &name);
		checked += count;
		failures.append(&mut found);
	}
	// The rest of fontdue's mistakes are rare (a fill leaking along a row,
	// round dots drawn with stray quadratics), so print disagreements for
	// review and fail when more than one glyph in 10,000 disagrees.
	for failure in &failures {
		eprintln!("{failure}");
	}
	eprintln!("{checked} glyphs compared, {} disagree", failures.len());
	assert!(checked > 0 && failures.len() * 10_000 <= checked);
}
