//! Relative positioning (HPR, VPR, HPB, VPB) and the screen alignment
//! pattern (DECALN).
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn row(term: &Term, y: usize) -> String {
	term.grid[y].iter().map(|c| c.ch).collect()
}

#[test]
fn hpr_moves_right_and_stops_at_the_last_column() {
	let mut t = terminal(10, 4);
	send(&mut t, "\x1b[2;3H\x1b[4a");
	assert_eq!((t.cx, t.cy), (6, 1));
	send(&mut t, "\x1b[a");
	assert_eq!(t.cx, 7, "the default count is 1");
	send(&mut t, "\x1b[0a");
	assert_eq!(t.cx, 8, "0 also means 1");
	send(&mut t, "\x1b[99a");
	assert_eq!((t.cx, t.cy), (9, 1));
}

#[test]
fn vpr_moves_down_keeping_the_column() {
	let mut t = terminal(10, 6);
	send(&mut t, "\x1b[1;5H\x1b[2e");
	assert_eq!((t.cx, t.cy), (4, 2));
	send(&mut t, "\x1b[e");
	assert_eq!((t.cx, t.cy), (4, 3));
	send(&mut t, "\x1b[99e");
	assert_eq!((t.cx, t.cy), (4, 5));
}

#[test]
fn vpr_in_origin_mode_stops_at_the_bottom_margin() {
	let mut t = terminal(10, 8);
	send(&mut t, "\x1b[2;5r\x1b[?6h\x1b[99e");
	assert_eq!(t.cy, 4, "row 5 is the bottom margin");
}

#[test]
fn vpr_below_the_bottom_margin_reaches_the_last_row() {
	let mut t = terminal(10, 8);
	send(&mut t, "\x1b[2;5r\x1b[7;1H\x1b[9e");
	assert_eq!(t.cy, 7);
}

#[test]
fn hpb_and_vpb_move_left_and_up() {
	let mut t = terminal(10, 6);
	send(&mut t, "\x1b[5;8H\x1b[3j\x1b[2k");
	assert_eq!((t.cx, t.cy), (4, 2));
	send(&mut t, "\x1b[j\x1b[k");
	assert_eq!((t.cx, t.cy), (3, 1));
	send(&mut t, "\x1b[99j\x1b[99k");
	assert_eq!((t.cx, t.cy), (0, 0));
}

#[test]
fn vpb_in_origin_mode_stops_at_the_top_margin() {
	let mut t = terminal(10, 8);
	send(&mut t, "\x1b[3;6r\x1b[?6h\x1b[4;1H\x1b[99k");
	assert_eq!(t.cy, 2);
}

#[test]
fn relative_moves_cancel_a_pending_wrap() {
	let mut t = terminal(4, 3);
	send(&mut t, "abcd\x1b[eX");
	assert_eq!(row(&t, 0), "abcd");
	assert_eq!(row(&t, 1), "   X", "no wrap to column 0 after VPR");
	send(&mut t, "\x1b[3;1Hwxyz\x1b[jQ");
	assert_eq!(row(&t, 2), "wxQz");
}

#[test]
fn decaln_fills_the_screen_with_e_and_homes_the_cursor() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[2;3r\x1b[31;44;4mhello\x1b[3;4H\x1b#8");
	for y in 0..3 {
		assert_eq!(row(&t, y), "EEEEE");
	}
	assert_eq!((t.cx, t.cy), (0, 0));
	assert_eq!((t.scroll_top, t.scroll_bottom), (0, 2), "margins are reset");
}

#[test]
fn decaln_cells_use_the_default_rendition() {
	let mut t = terminal(4, 2);
	send(&mut t, "\x1b[31;44;1;4;7m\x1b#8");
	let cell = t.grid[1][3];
	assert_eq!(cell.ch, 'E');
	assert_eq!(
		(cell.fg, cell.bg, cell.attrs),
		(t.default_fg, t.default_bg, 0)
	);
	assert_eq!(cell.width, 1);
}

#[test]
fn decaln_replaces_wide_characters_and_cancels_wrap() {
	let mut t = terminal(4, 2);
	send(&mut t, "ab世\x1b#8");
	assert_eq!(row(&t, 0), "EEEE");
	assert!(t.grid[0].iter().all(|c| c.width == 1));
	assert!(!t.wrap_pending);
	send(&mut t, "Z");
	assert_eq!(row(&t, 0), "ZEEE");
}

#[test]
fn other_esc_hash_finals_leave_the_screen_alone() {
	let mut t = terminal(4, 2);
	send(&mut t, "ab\x1b#9\x1b#0");
	assert_eq!(row(&t, 0), "ab  ");
	assert_eq!(t.cx, 2);
}
