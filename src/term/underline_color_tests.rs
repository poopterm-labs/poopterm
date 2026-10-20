//! SGR 58/59 underline color: kept per cell next to marks and hyperlinks.
use crate::{
	parser::Parser,
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The underline color of each cell in row 0, up to `len`.
fn colors(term: &Term, len: usize) -> Vec<Option<u32>> {
	term.grid[0][..len]
		.iter()
		.map(|cell| term.extras.underline_color(cell.extra))
		.collect()
}

#[test]
fn every_form_of_sgr_58_sets_the_color() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b[58:2::1:2:3mA\x1b[58:2:4:5:6mB\x1b[58;2;7;8;9mC\x1b[58;5;196mD\x1b[58:5:21mE",
	);
	assert_eq!(
		colors(&t, 5),
		[
			Some(0x010203),
			Some(0x040506),
			Some(0x070809),
			Some(t.indexed_color(196)),
			Some(t.indexed_color(21)),
		]
	);
}

#[test]
fn black_is_a_color_not_the_absence_of_one() {
	let mut t = terminal();
	send(&mut t, "\x1b[58:2::0:0:0mA");
	assert_eq!(colors(&t, 1), [Some(0)]);
	assert_ne!(t.grid[0][0].extra, super::NO_EXTRA);
}

#[test]
fn sgr_59_and_sgr_0_clear_it_and_unset_cells_stay_plain() {
	let mut t = terminal();
	send(
		&mut t,
		"a\x1b[58:2::1:2:3mb\x1b[59mc\x1b[58:5:1md\x1b[me\x1b[58:5:1mf\x1b[0mg",
	);
	let red = Some(t.indexed_color(1));
	assert_eq!(
		colors(&t, 7),
		[None, Some(0x010203), None, red, None, red, None]
	);
	for col in [0, 2, 4, 6] {
		assert_eq!(t.grid[0][col].extra, super::NO_EXTRA, "column {col}");
	}
}

#[test]
fn malformed_sgr_58_leaves_the_color_alone() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b[58:2::1:2:3m\x1b[58m\x1b[58:9:1mA\x1b[58;2;1mB",
	);
	assert_eq!(colors(&t, 2), [Some(0x010203); 2]);
}

#[test]
fn other_styles_do_not_clear_it() {
	let mut t = terminal();
	send(&mut t, "\x1b[4;58:2::1:2:3mA\x1b[24;31;44;1mB\x1b[39;49mC");
	assert_eq!(colors(&t, 3), [Some(0x010203); 3]);
}

#[test]
fn it_combines_with_hyperlinks_in_either_order_and_outlives_them() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]8;;https://a\x07\x1b[58:2::1:2:3mA\x1b]8;;\x07B\x1b]8;;https://b\x07C\x1b[59mD",
	);
	assert_eq!(
		colors(&t, 4),
		[Some(0x010203), Some(0x010203), Some(0x010203), None]
	);
	let links: Vec<u16> = t.grid[0][..4]
		.iter()
		.map(|cell| t.extras.hyperlink(cell.extra))
		.collect();
	assert_eq!(links[1], 0, "the link ended but the color stayed");
	assert!(links[0] != 0 && links[2] != 0 && links[3] == links[2]);
	assert_ne!(links[0], links[2]);
}

#[test]
fn combining_marks_keep_the_cell_color() {
	let mut t = terminal();
	send(&mut t, "\x1b[58:2::1:2:3me\u{301}\x1b[59mx\u{301}");
	assert_eq!(colors(&t, 2), [Some(0x010203), None]);
	assert_eq!(t.extras.marks(t.grid[0][0].extra), ['\u{301}']);
	assert_eq!(t.extras.marks(t.grid[0][1].extra), ['\u{301}']);
}

#[test]
fn erasing_does_not_paint_the_color_into_blanks() {
	let mut t = terminal();
	send(&mut t, "\x1b[58:2::1:2:3mab\x1b[K\r\x1b[X\x1b[2;1H\x1b[J");
	assert_eq!(colors(&t, 2), [None, Some(0x010203)]);
	assert!(t.grid[1].iter().all(|cell| cell.extra == super::NO_EXTRA));
}

#[test]
fn decsc_saves_it_and_decrc_puts_it_back() {
	let mut t = terminal();
	send(&mut t, "\x1b[58:2::1:2:3m\x1b7\x1b[59mA\x1b8B");
	assert_eq!(
		colors(&t, 1),
		[Some(0x010203)],
		"B overwrote A with the saved color"
	);
	send(&mut t, "\x1b[59m\x1b7\x1b[58:5:1m\x1b8C");
	assert_eq!(
		colors(&t, 2),
		[Some(0x010203), None],
		"C was printed without one"
	);
}

#[test]
fn decrqss_reports_it() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b[4;58:5:21;58:2::18:52:86m\x1bP$qm\x1b\\\x1b[59m\x1bP$qm\x1b\\",
	);
	assert_eq!(
		std::mem::take(&mut t.responses),
		[
			b"\x1bP1$r0;4;58:2::18:52:86m\x1b\\".to_vec(),
			b"\x1bP1$r0;4m\x1b\\".to_vec(),
		]
	);
}

#[test]
fn frames_carry_it_and_compare_rows_by_it() {
	let mut t = terminal();
	send(&mut t, "\x1b[4;58:2::1:2:3mA\x1b[59mB");
	let mut first = Frame::default();
	t.snapshot(&mut first);
	let row = first.row(0);
	assert_eq!(first.underline_color(&row[0]), Some(0x010203));
	assert_eq!(first.underline_color(&row[1]), None);
	assert!(first.row_matches(0, &first, 0));

	send(&mut t, "\r\x1b[58:2::9:9:9mA");
	let mut second = Frame::default();
	t.snapshot(&mut second);
	assert_eq!(second.underline_color(&second.row(0)[0]), Some(0x090909));
	assert!(!second.row_matches(0, &first, 0), "only the color changed");
	assert!(second.row_matches(1, &first, 1));
}

#[test]
fn soft_reset_clears_it_with_the_other_styles() {
	let mut t = terminal();
	send(&mut t, "\x1b[31;58:2::1:2:3m\x1b[!pA");
	assert_eq!(colors(&t, 1), [None]);
	assert_eq!((t.grid[0][0].fg, t.fg_id), (t.default_fg, 0));
}
