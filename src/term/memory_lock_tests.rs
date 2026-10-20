//! HP memory lock (`ESC l`) and unlock (`ESC m`), advertised as meml and memu.
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

fn rows(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|y| {
			term.grid[y]
				.iter()
				.map(|c| c.ch)
				.collect::<String>()
				.trim_end()
				.to_owned()
		})
		.collect()
}

#[test]
fn lines_above_the_cursor_stay_while_the_rest_scrolls() {
	let mut t = terminal(4, 5);
	send(&mut t, "a\r\nb\r\n\x1blc\r\nd\r\ne\r\nf\r\ng");
	assert_eq!(rows(&t), ["a", "b", "e", "f", "g"]);
	assert_eq!(t.scroll_top, 2, "the lock starts at the cursor's row");
}

#[test]
fn locking_leaves_the_cursor_where_it_was() {
	let mut t = terminal(6, 5);
	send(&mut t, "\x1b[3;4H\x1bl");
	assert_eq!((t.cx, t.cy), (3, 2));
	assert_eq!(t.scroll_bottom, 4, "only the top margin moves");
}

#[test]
fn unlocking_scrolls_the_whole_screen_again() {
	let mut t = terminal(4, 4);
	send(&mut t, "a\r\n\x1blb\r\nc\r\nd\x1bm\r\ne");
	assert_eq!(rows(&t), ["b", "c", "d", "e"]);
	assert_eq!(t.scroll_top, 0);
}

#[test]
fn a_lock_on_the_bottom_margin_is_ignored() {
	let mut t = terminal(4, 4);
	send(&mut t, "\x1b[4H\x1bl");
	assert_eq!(t.scroll_top, 0, "a region needs two lines");
	send(&mut t, "\x1b[2;3r\x1b[3H\x1bl");
	assert_eq!(t.scroll_top, 1, "nor on the bottom of a set region");
}

#[test]
fn unlocking_without_a_lock_keeps_the_margins() {
	let mut t = terminal(4, 6);
	send(&mut t, "\x1b[2;5r\x1bm");
	assert_eq!((t.scroll_top, t.scroll_bottom), (1, 4));
}

#[test]
fn setting_margins_replaces_a_lock() {
	let mut t = terminal(4, 6);
	send(&mut t, "\x1b[3H\x1bl\x1b[2;5r\x1bm");
	assert_eq!(
		(t.scroll_top, t.scroll_bottom),
		(1, 4),
		"unlock does not undo margins set after the lock"
	);
}

#[test]
fn a_lock_inside_margins_keeps_their_bottom() {
	let mut t = terminal(4, 6);
	send(&mut t, "\x1b[1;4ra\r\nb\x1bl\r\nc\r\nd\r\ne");
	assert_eq!(rows(&t), ["a", "c", "d", "e", "", ""]);
	send(&mut t, "\x1bm");
	assert_eq!((t.scroll_top, t.scroll_bottom), (0, 3));
}

#[test]
fn reset_drops_the_lock() {
	let mut t = terminal(4, 4);
	send(&mut t, "\x1b[2H\x1bl\x1bc");
	assert_eq!(t.scroll_top, 0);
	send(&mut t, "\x1b[2;3r\x1bm");
	assert_eq!(t.scroll_top, 1, "no lock is left to undo");
}
