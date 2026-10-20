//! Reverse wraparound (DECSET 45, advertised as `bw`): backspace in the
//! first column moves to the end of the line above.
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

#[test]
fn backspace_in_the_first_column_goes_to_the_end_of_the_line_above() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[3H\x08");
	assert_eq!((t.cx, t.cy), (4, 1));
	send(&mut t, "\x08");
	assert_eq!((t.cx, t.cy), (3, 1), "elsewhere it moves one column left");
}

#[test]
fn it_is_on_by_default_and_reported_by_decrqm() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[?45$p");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?45;1$y");
	send(&mut t, "\x1b[?45l\x1b[?45$p");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?45;2$y");
}

#[test]
fn turning_it_off_keeps_backspace_on_the_line() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[?45l\x1b[2H\x08");
	assert_eq!((t.cx, t.cy), (0, 1));
	send(&mut t, "\x1b[?45h\x08");
	assert_eq!((t.cx, t.cy), (4, 0), "and turning it on again restores it");
}

#[test]
fn it_needs_autowrap() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[?7l\x1b[2H\x08");
	assert_eq!((t.cx, t.cy), (0, 1));
}

#[test]
fn the_top_line_does_not_wrap_to_the_bottom() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x08");
	assert_eq!((t.cx, t.cy), (0, 0));
}

#[test]
fn the_top_margin_stops_it_inside_the_region() {
	let mut t = terminal(5, 6);
	send(&mut t, "\x1b[3;5r\x1b[3H\x08");
	assert_eq!((t.cx, t.cy), (0, 2));
	send(&mut t, "\x1b[2H\x08");
	assert_eq!((t.cx, t.cy), (4, 0), "above the region it still wraps");
}

#[test]
fn xtsave_and_xtrestore_keep_it() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[?45s\x1b[?45l\x1b[?45r\x1b[2H\x08");
	assert_eq!((t.cx, t.cy), (4, 0));
}

#[test]
fn a_full_reset_turns_it_back_on() {
	let mut t = terminal(5, 3);
	send(&mut t, "\x1b[?45l\x1bc\x1b[2H\x08");
	assert_eq!((t.cx, t.cy), (4, 0));
}

#[test]
fn erasing_a_wrapped_word_walks_back_across_the_line_break() {
	let mut t = terminal(4, 3);
	send(&mut t, "abcdef\x08\x08\x08");
	assert_eq!((t.cx, t.cy), (3, 0));
	send(&mut t, " ");
	let row: String = t.grid[0].iter().map(|c| c.ch).collect();
	assert_eq!(row, "abc ");
}
