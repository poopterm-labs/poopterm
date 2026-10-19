//! XTWINOPS (`CSI Ps ; … t`): size reports and the title stack. Requests
//! to move, resize or raise the window, and to report the title, are
//! ignored.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	let mut term = Term::new(80, 24, Theme::from_kind(ThemeKind::Poop), "/".into());
	(term.cell_width, term.cell_height) = (9, 19);
	term
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn sizes_are_reported() {
	let mut t = terminal();
	send(&mut t, "\x1b[14t\x1b[14;2t\x1b[16t\x1b[18t\x1b[19t\x1b[11t");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b[4;456;720t",
			"\x1b[4;456;720t",
			"\x1b[6;19;9t",
			"\x1b[8;24;80t",
			"\x1b[9;24;80t",
			"\x1b[1t",
		]
	);
}

#[test]
fn reports_keep_a_pending_wrap() {
	let mut t = terminal();
	send(&mut t, &"x".repeat(80));
	send(&mut t, "\x1b[18t");
	assert!(t.wrap_pending);
}

#[test]
fn window_changes_and_title_reports_are_ignored() {
	let mut t = terminal();
	send(&mut t, "\x1b]2;secret\x07");
	for op in [
		"1",
		"2",
		"3;10;10",
		"4;100;100",
		"5",
		"6",
		"7",
		"8;10;10",
		"9;1",
		"10;1",
		"13",
		"15",
		"20",
		"21",
		"24",
		"99",
	] {
		send(&mut t, &format!("\x1b[{op}t"));
	}
	assert!(t.responses.is_empty());
	assert_eq!((t.cols, t.rows), (80, 24));
}

#[test]
fn titles_are_pushed_and_popped() {
	let mut t = terminal();
	send(&mut t, "\x1b]0;one\x07\x1b[22;0t\x1b]0;two\x07");
	send(&mut t, "\x1b[23;0t");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("one", "one"));
	// The window title and the icon name have stacks of their own.
	send(&mut t, "\x1b[22;2t\x1b]0;three\x07\x1b[23;1t");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("three", "three"));
	send(&mut t, "\x1b[23;2t");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("one", "three"));
	// A missing second parameter means both; popping an empty stack does
	// nothing.
	send(&mut t, "\x1b[22t\x1b]0;four\x07\x1b[23t\x1b[23t");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("one", "three"));
}

#[test]
fn popping_a_title_redraws_the_tab() {
	let mut t = terminal();
	send(&mut t, "\x1b]2;one\x07\x1b[22;2t\x1b]2;two\x07");
	let generation = t.title_generation;
	send(&mut t, "\x1b[23;2t");
	assert_eq!(t.title_generation, generation.wrapping_add(1));
}

#[test]
fn the_title_stack_is_bounded() {
	let mut t = terminal();
	for index in 0..40 {
		send(&mut t, &format!("\x1b]2;t{index}\x07\x1b[22;2t"));
	}
	for _ in 0..40 {
		send(&mut t, "\x1b[23;2t");
	}
	// The oldest titles made room for the newest.
	assert_eq!(t.title, "t30");
}
