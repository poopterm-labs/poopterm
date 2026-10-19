//! iTerm2's tab color (OSC 6;1) and session status (OSC 21337).
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

#[test]
fn tab_colors_are_set_one_channel_at_a_time() {
	let mut t = terminal();
	send(&mut t, "\x1b]6;1;bg;red;brightness;255\x07");
	assert_eq!(t.tab_style.color, Some(0xff0000));
	assert_eq!(t.title_generation, 1);
	send(
		&mut t,
		"\x1b]6;1;bg;green;brightness;0\x07\x1b]6;1;bg;blue;brightness;128\x1b\\",
	);
	assert_eq!(t.tab_style.color, Some(0xff0080));
	assert_eq!(t.title_generation, 2);
	send(&mut t, "\x1b]6;1;bg;red;brightness;1\x07");
	assert_eq!(t.tab_style.color, Some(0x010080));
	send(&mut t, "\x1b]6;1;bg;red;brightness;255\x07");
	// Bad channels and values change nothing.
	for bad in [
		"1;bg;red;brightness;256",
		"1;bg;red;brightness;-1",
		"1;bg;red;brightness;",
		"1;bg;pink;brightness;1",
		"1;bg;red;contrast;1",
		"2;bg;red;brightness;1",
		"1;bg;*",
	] {
		send(&mut t, &format!("\x1b]6;{bad}\x07"));
		assert_eq!(t.tab_style.color, Some(0xff0080), "{bad}");
	}
	assert_eq!(t.title_generation, 4);
	send(&mut t, "\x1b]6;1;bg;*;default\x07");
	assert_eq!(t.tab_style.color, None);
	assert_eq!(t.title_generation, 5);
}

#[test]
fn xterm_special_color_modes_still_work() {
	let mut t = terminal();
	send(&mut t, "\x1b]6;1;1\x07");
	assert!(t.colors.special_modes[1]);
	assert_eq!(t.tab_style.color, None);
}

#[test]
fn session_status_sets_and_clears_fields() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]21337;indicator=#00ff00;status=building;status-color=#FFA500\x07",
	);
	assert_eq!(t.tab_style.indicator, Some(0x00ff00));
	assert_eq!(t.tab_style.status.as_deref(), Some("building"));
	assert_eq!(t.tab_style.status_color, Some(0xffa500));
	assert_eq!(t.title_generation, 1);
	// Omitted fields stay; empty ones clear.
	send(&mut t, "\x1b]21337;status=\x07");
	assert_eq!(t.tab_style.status, None);
	assert_eq!(t.tab_style.indicator, Some(0x00ff00));
	send(&mut t, "\x1b]21337;indicator=;status-color=\x07");
	assert_eq!(t.tab_style.indicator, None);
	assert_eq!(t.tab_style.status_color, None);
	assert_eq!(t.title_generation, 3);
	// Nothing changed, nothing redrawn.
	send(&mut t, "\x1b]21337;status=\x07");
	assert_eq!(t.title_generation, 3);
}

#[test]
fn session_status_is_bounded_and_validated() {
	let mut t = terminal();
	send(
		&mut t,
		&format!("\x1b]21337;status=a\u{85}{}\x07", "b".repeat(100)),
	);
	assert_eq!(t.tab_style.status, Some(format!("a{}", "b".repeat(63))));
	send(
		&mut t,
		"\x1b]21337;indicator=#12345\x07\x1b]21337;indicator=123456\x07",
	);
	send(
		&mut t,
		"\x1b]21337;indicator=#gggggg\x07\x1b]21337;indicator=#+12345\x07\x1b]21337;bogus=1;indicator\x07",
	);
	assert_eq!(t.tab_style.indicator, None);
}
