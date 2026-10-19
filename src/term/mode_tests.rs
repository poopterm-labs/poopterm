//! Terminal modes: setting, resetting and reporting them.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 5, Theme::from_kind(ThemeKind::Poop), "/".into())
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
fn deckpam_and_deckpnm_switch_the_keypad() {
	let mut t = terminal();
	assert!(!t.application_keypad, "the keypad starts numeric");
	send(&mut t, "\x1b=");
	assert!(t.application_keypad);
	send(&mut t, "\x1b>");
	assert!(!t.application_keypad);
}

#[test]
fn decnkm_is_the_same_keypad_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[?66h");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?66$p");
	send(&mut t, "\x1b>\x1b[?66$p");
	assert_eq!(replies(&mut t), ["\x1b[?66;1$y", "\x1b[?66;2$y"]);
	send(&mut t, "\x1b=\x1b[?66l");
	assert!(!t.application_keypad);
}

#[test]
fn soft_and_hard_resets_return_the_keypad_to_numeric() {
	let mut t = terminal();
	send(&mut t, "\x1b=\x1b[!p");
	assert!(!t.application_keypad, "DECSTR");
	send(&mut t, "\x1b=\x1bc");
	assert!(!t.application_keypad, "RIS");
}

#[test]
fn keypad_mode_survives_cursor_save_and_screen_switches() {
	let mut t = terminal();
	send(&mut t, "\x1b7\x1b=\x1b8");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?1049h");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?1049l");
	assert!(t.application_keypad);
}

#[test]
fn keypad_escapes_with_intermediates_are_other_sequences() {
	let mut t = terminal();
	send(&mut t, "\x1b(=\x1b#=");
	assert!(!t.application_keypad);
}

#[test]
fn keypad_mode_does_not_move_the_cursor_or_cancel_wrap() {
	let mut t = terminal();
	send(&mut t, "\x1b[1;20HX\x1b=");
	assert_eq!((t.cx, t.cy), (19, 0));
	assert!(t.wrap_pending);
}

#[test]
fn decnkm_combines_with_other_modes_in_one_sequence() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1;66h");
	assert!(t.application_cursor && t.application_keypad);
	send(&mut t, "\x1b[?66;1l");
	assert!(!t.application_cursor && !t.application_keypad);
}
