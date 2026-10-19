//! DECARM, DECBKM and xterm's Meta, NumLock and Delete modes, as programs
//! set, query, save and restore them.
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

fn replies(term: &mut Term) -> String {
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn set_and_reset_change_each_flag() {
	let mut t = terminal();
	send(&mut t, "\x1b[?8l\x1b[?67h\x1b[?1034l\x1b[?1035l");
	send(&mut t, "\x1b[?1036l\x1b[?1037h\x1b[?1039h");
	let k = t.keyboard;
	assert!(!k.auto_repeat && k.backarrow_bs && !k.eight_bit_meta && !k.numlock);
	assert!(!k.meta_escape && k.delete_is_del && k.alt_escape);
	send(&mut t, "\x1b[?8;67;1034;1035;1036;1037;1039h");
	let k = t.keyboard;
	assert!(k.auto_repeat && k.backarrow_bs && k.eight_bit_meta && k.numlock);
	assert!(k.meta_escape && k.delete_is_del && k.alt_escape);
}

#[test]
fn decrqm_reports_the_keyboard_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?8$p\x1b[?67$p\x1b[?1036$p\x1b[?1039$p");
	assert_eq!(
		replies(&mut t),
		"\x1b[?8;1$y\x1b[?67;2$y\x1b[?1036;1$y\x1b[?1039;2$y"
	);
	send(&mut t, "\x1b[?67h\x1b[?8l\x1b[?67$p\x1b[?8$p\x1b[?1038$p");
	assert_eq!(replies(&mut t), "\x1b[?67;1$y\x1b[?8;2$y\x1b[?1038;0$y");
}

#[test]
fn xtsave_and_xtrestore_cover_the_keyboard_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?67h\x1b[?67;1036s\x1b[?67l\x1b[?1036l");
	assert!(!t.keyboard.backarrow_bs && !t.keyboard.meta_escape);
	send(&mut t, "\x1b[?67;1036r");
	assert!(t.keyboard.backarrow_bs && t.keyboard.meta_escape);
}

#[test]
fn hard_reset_restores_the_initial_keyboard_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?8l\x1b[?67h\x1b[?1036l\x1b[?1037h\x1bc");
	assert_eq!(t.keyboard, Default::default());
}

#[test]
fn keyboard_modes_leave_the_screen_alone() {
	let mut t = terminal();
	send(&mut t, "\x1b[1;20HX\x1b[?8l\x1b[?67h\x1b[?1035l\x1b[?1039h");
	assert_eq!((t.cx, t.cy), (19, 0));
	assert!(t.wrap_pending);
	assert!(t.auto_wrap && t.cursor_visible, "no other mode changed");
}

#[test]
fn soft_reset_leaves_the_keyboard_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?8l\x1b[?67h\x1b[?1036l\x1b[!p");
	assert!(!t.keyboard.auto_repeat && t.keyboard.backarrow_bs && !t.keyboard.meta_escape);
}
