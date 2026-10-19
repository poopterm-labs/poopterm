//! Pixel equivalence and timing for highlight damage, including scrolling.
use super::{Canvas, DrawKey, Plan};
use crate::{
	parser::Parser,
	render::{
		glyphs::FontSet,
		grid::{GridView, draw_grid},
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
	ui::selection::CellPoint,
};

const KEY: DrawKey = DrawKey {
	session: 1,
	focused: true,
	selection: None,
	search_match: None,
	overlay: false,
};

fn fonts() -> FontSet {
	FontSet::new(
		fontdue::Font::from_bytes(
			include_bytes!("../../assets/font.ttf") as &[u8],
			fontdue::FontSettings::default(),
		)
		.unwrap(),
		Vec::new(),
	)
}

fn draw(
	pixels: &mut [u32],
	fonts: &mut FontSet,
	frame: &Frame,
	key: DrawKey,
	rows: Option<&[bool]>,
) {
	let bg = Theme::from_kind(ThemeKind::Poop).bg;
	let width = frame.cols * 10;
	let height = frame.rows * 20;
	let mut painter = Painter::new(pixels, width, height);
	for row in 0..frame.rows {
		if rows.is_none_or(|rows| rows[row]) {
			painter.rect(0, row * 20, width, 20, bg);
		}
	}
	let view = GridView {
		frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 16.0,
		ascent: 15,
		scale: 1,
		default_bg: bg,
		link_color: 0x123456,
		selection_colors: (0x111111, 0xffffff),
		match_colors: (0, 0xffff00),
		selection: key.selection,
		search_match: key.search_match,
		focused: key.focused,
	};
	draw_grid(&mut painter, fonts, &view, rows);
}

fn update(canvas: &mut Canvas, fonts: &mut FontSet, frame: &mut Frame, key: DrawKey) {
	let width = frame.cols * 10;
	let height = frame.rows * 20;
	let plan = canvas.plan(frame, key, (width, height), true);
	let rows = match &plan {
		Plan::Full => None,
		Plan::Partial(damage) => {
			if damage.scroll > 0 {
				canvas.pixels.copy_within(damage.scroll * 20 * width.., 0);
			}
			Some(damage.rows.as_slice())
		}
	};
	draw(&mut canvas.pixels, fonts, frame, key, rows);
	canvas.remember(frame, key);
}

#[test]
fn highlight_damage_and_scrolling_match_a_full_redraw_pixel_for_pixel() {
	let mut term = Term::new(12, 5, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	let mut fonts = fonts();
	let mut canvas = Canvas::new();
	let range = |row, col| Some((CellPoint { row, col: 0 }, CellPoint { row, col }));
	for (input, selection, search_match) in [
		("abc猫e\u{301}\r\ndef\r\nghi", None, None),
		("", range(1, 2), None),
		("", range(1, 4), None),
		("\r\nnew\r\nlast\r\nscroll", range(1, 4), range(4, 3)),
		("", None, range(2, 4)),
		("", None, None),
		("\x1b[H\x1b[K猫", range(2, 2), None),
	] {
		parser.advance(&mut term, input.as_bytes());
		let key = DrawKey {
			selection,
			search_match,
			..KEY
		};
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut expected = vec![0; frame.cols * 10 * frame.rows * 20];
		draw(&mut expected, &mut fonts, &frame, key, None);
		update(&mut canvas, &mut fonts, &mut frame, key);
		assert_eq!(canvas.pixels, expected, "input {input:?}, key {key:?}");
	}
}

#[test]
#[ignore = "rendering benchmark; run explicitly in release mode"]
fn highlight_rendering_throughput() {
	let mut term = Term::new(100, 30, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, "hello 猫 e\u{301}\r\n".repeat(30).as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	let mut fonts = fonts();
	for full in [true, false] {
		let mut canvas = Canvas::new();
		let start = std::time::Instant::now();
		for index in 0..300 {
			let row = index % 30 + frame.first_row;
			let key = DrawKey {
				selection: Some((CellPoint { row, col: 1 }, CellPoint { row, col: 5 })),
				..KEY
			};
			if full {
				canvas.invalidate();
			}
			update(&mut canvas, &mut fonts, &mut frame.clone(), key);
		}
		println!(
			"highlight full={full}: {:?} for 300 frames",
			start.elapsed()
		);
	}
}
