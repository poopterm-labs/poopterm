//! OSC 22 pointer shapes: setting, the per-screen stacks, xterm names and
//! queries.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn current(term: &Term) -> Option<&'static str> {
	term.pointer_shape()
}

fn replies(term: &mut Term) -> Vec<String> {
	term
		.responses
		.drain(..)
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn shapes_are_set_pushed_and_popped() {
	let mut t = terminal();
	assert_eq!(current(&t), None);
	send(&mut t, "\x1b]22;pointer\x1b\\");
	assert_eq!(current(&t), Some("pointer"));
	// Setting replaces the top; xterm's cursor font names are aliases.
	send(&mut t, "\x1b]22;=watch\x1b\\");
	assert_eq!(current(&t), Some("wait"));
	send(&mut t, "\x1b]22;>text,crosshair\x1b\\");
	assert_eq!(current(&t), Some("crosshair"));
	send(&mut t, "\x1b]22;<\x1b\\\x1b]22;<ignored\x1b\\");
	assert_eq!(current(&t), Some("wait"));
	// Unknown names change nothing; an empty set resets.
	send(&mut t, "\x1b]22;no-such-shape\x1b\\");
	assert_eq!(current(&t), Some("wait"));
	send(&mut t, "\x1b]22;\x1b\\\x1b]22;<\x1b\\");
	assert_eq!(current(&t), None);
}

#[test]
fn screens_have_their_own_stacks_and_resets_empty_both() {
	let mut t = terminal();
	send(&mut t, "\x1b]22;text\x1b\\\x1b[?1049h");
	assert_eq!(current(&t), None);
	send(&mut t, "\x1b]22;grab\x1b\\");
	assert_eq!(current(&t), Some("grab"));
	send(&mut t, "\x1b[?1049l");
	assert_eq!(current(&t), Some("text"));
	send(&mut t, "\x1bc");
	assert_eq!(current(&t), None);
}

#[test]
fn stacks_keep_the_most_recent_shapes() {
	let mut t = terminal();
	let names = ["alias", "cell", "copy"].repeat(10).join(",");
	send(&mut t, &format!("\x1b]22;>{names}\x1b\\"));
	for _ in 0..super::pointer::MAX_STACK - 1 {
		send(&mut t, "\x1b]22;<\x1b\\");
	}
	assert!(current(&t).is_some());
	send(&mut t, "\x1b]22;<\x1b\\");
	assert_eq!(current(&t), None);
}

#[test]
fn queries_report_support_and_the_current_shape() {
	let mut t = terminal();
	send(&mut t, "\x1b]22;?__current__\x07");
	send(
		&mut t,
		"\x1b]22;help\x1b\\\x1b]22;?__current__,pointer,no-such-name,hand2,__default__,__grabbed__\x1b\\",
	);
	assert_eq!(
		replies(&mut t),
		["\x1b]22;0\x07", "\x1b]22;help,1,0,1,default,default\x1b\\"]
	);
}

#[test]
fn every_shape_has_a_window_cursor() {
	for name in super::pointer::NAMES {
		assert!(name.parse::<winit::window::CursorIcon>().is_ok(), "{name}");
	}
}
