//! Reverse screen video (DECSCNM, DECSET 5), which the terminfo `flash`
//! capability turns on briefly: frames swap every cell's colors.
use crate::{
	parser::Parser,
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn colors(term: &Term) -> Vec<(u32, u32)> {
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	(0..frame.rows)
		.flat_map(|row| frame.row(row).to_vec())
		.map(|cell| (cell.fg, cell.bg))
		.collect()
}

#[test]
fn every_cell_swaps_its_colors_blank_or_not() {
	let mut t = terminal();
	send(&mut t, "\x1b[31;42mx");
	let normal = colors(&t);
	send(&mut t, "\x1b[?5h");
	let reversed = colors(&t);
	assert_eq!(reversed.len(), 8);
	for (a, b) in normal.iter().zip(&reversed) {
		assert_eq!((a.1, a.0), *b);
	}
	assert_eq!(reversed[1], (t.default_bg, t.default_fg), "blanks too");
}

#[test]
fn turning_it_off_restores_the_colors() {
	let mut t = terminal();
	send(&mut t, "\x1b[7mx");
	let normal = colors(&t);
	send(&mut t, "\x1b[?5h\x1b[?5l");
	assert_eq!(colors(&t), normal);
}

#[test]
fn reverse_video_cells_look_normal_on_a_reversed_screen() {
	let mut t = terminal();
	send(&mut t, "\x1b[7mx\x1b[?5h");
	assert_eq!(colors(&t)[0], (t.default_fg, t.default_bg));
}

#[test]
fn the_grid_keeps_its_colors() {
	let mut t = terminal();
	send(&mut t, "\x1b[?5hx");
	let cell = t.grid[0][0];
	assert_eq!((cell.fg, cell.bg), (t.default_fg, t.default_bg));
	send(&mut t, "\x1b[?5l");
	assert_eq!(colors(&t)[0], (t.default_fg, t.default_bg));
}

#[test]
fn history_rows_in_view_are_reversed_too() {
	let mut t = terminal();
	send(&mut t, "a\r\nb\r\nc\x1b[?5h");
	t.scroll_offset = 1;
	let reversed = colors(&t);
	assert!(
		reversed
			.iter()
			.all(|&colors| colors == (t.default_bg, t.default_fg))
	);
}

#[test]
fn decrqm_and_xtsave_know_the_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[?5$p");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?5;2$y");
	send(&mut t, "\x1b[?5h\x1b[?5$p");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?5;1$y");
	send(&mut t, "\x1b[?5s\x1b[?5l\x1b[?5r\x1b[?5$p");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?5;1$y");
}

#[test]
fn a_full_reset_clears_it_and_a_soft_reset_keeps_it() {
	let mut t = terminal();
	send(&mut t, "\x1b[?5h\x1b[!p");
	assert_eq!(colors(&t)[0], (t.default_bg, t.default_fg));
	send(&mut t, "\x1bc");
	assert_eq!(colors(&t)[0], (t.default_fg, t.default_bg));
}
