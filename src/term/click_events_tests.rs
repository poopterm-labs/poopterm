//! kitty's OSC 133 `click_events`: clicks in the prompt reach the shell as
//! SGR mouse reports instead of arrow keys.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The press and release of the left button at 1-based `x`, `y`.
fn click(x: usize, y: usize) -> Vec<u8> {
	format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").into_bytes()
}

#[test]
fn absolute_click_events_report_screen_coordinates() {
	let mut t = terminal();
	// Output on row 0, then a two-line command under a prompt on row 1.
	send(
		&mut t,
		"out\r\n\x1b]133;A;click_events=1\x07$ \x1b]133;B\x07ab\r\ncd",
	);
	assert_eq!(t.prompt_click(2, 1), Some(click(2, 3)));
	// The prompt itself and the input's first line count too.
	assert_eq!(t.prompt_click(1, 0), Some(click(1, 2)));
	assert_eq!(t.prompt_click(1, 9), Some(click(10, 2)));
	// Rows below the cursor are the shell's to judge.
	assert_eq!(t.prompt_click(3, 4), Some(click(5, 4)));
	// Clicks above the prompt are not the shell's.
	assert_eq!(t.prompt_click(0, 1), None);
}

#[test]
fn relative_click_events_count_from_the_nearest_prompt_above() {
	let mut t = terminal();
	send(
		&mut t,
		"out\r\n\x1b]133;A;click_events=2\x07$ \x1b]133;B\x07if x\r\n\
		 \x1b]133;A;k=s\x07> \x1b]133;B\x07y",
	);
	// The secondary prompt on row 2 is on or above the click.
	assert_eq!(t.prompt_click(2, 3), Some(click(4, 1)));
	assert_eq!(t.prompt_click(3, 0), Some(click(1, 2)));
	// Above it, the primary prompt on row 1 counts.
	assert_eq!(t.prompt_click(1, 2), Some(click(3, 1)));
	assert_eq!(t.prompt_click(0, 2), None);
	// A new primary prompt forgets the secondary ones.
	send(&mut t, "\r\n\x1b]133;A;click_events=2\x07$ \x1b]133;B\x07");
	assert_eq!(t.prompt_click(3, 2), Some(click(3, 1)));
	assert_eq!(t.prompt_click(2, 2), None);
}

#[test]
fn click_events_take_the_place_of_arrow_keys() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]133;A;cl=line;click_events=1\x07$ \x1b]133;B\x07abc",
	);
	assert_eq!(t.prompt_click(0, 3), Some(click(4, 1)));
	// Unknown values fall back to what cl= asks for.
	send(
		&mut t,
		"\r\n\x1b]133;A;cl=line;click_events=3\x07$ \x1b]133;B\x07abc",
	);
	assert_eq!(t.prompt_click(1, 3).as_deref(), Some(&b"\x1b[D\x1b[D"[..]));
	// Without either, clicks stay with the terminal.
	send(
		&mut t,
		"\r\n\x1b]133;A;click_events=0\x07$ \x1b]133;B\x07abc",
	);
	assert_eq!(t.prompt_click(2, 3), None);
}

#[test]
fn click_events_need_a_prompt_on_the_live_screen() {
	let mut t = terminal();
	send(&mut t, "\x1b]133;A;click_events=1\x07$ \x1b]133;B\x07ls");
	assert_eq!(t.prompt_click(0, 2), Some(click(3, 1)));
	// While the command runs, clicks are not for the shell.
	send(&mut t, "\x1b]133;C\x07");
	assert_eq!(t.prompt_click(0, 2), None);
	// Nor on the alternate screen, or with the view scrolled back.
	send(
		&mut t,
		"\x1b]133;D;0\x07\r\n\x1b]133;A;click_events=1\x07$ \x1b]133;B\x07",
	);
	assert_eq!(t.prompt_click(1, 2), Some(click(3, 2)));
	send(&mut t, "\x1b[?1049h");
	assert_eq!(t.prompt_click(1, 2), None);
	send(&mut t, "\x1b[?1049l\r\n\r\n\r\n");
	t.scroll_view(1);
	assert_eq!(t.prompt_click(1, 2), None);
}

#[test]
fn rows_scrolled_into_history_keep_their_offsets() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]133;A;click_events=2\x07$ \x1b]133;B\x07a\r\nb\r\nc\r\nd\r\ne",
	);
	// The prompt scrolled off the top; the screen's rows 1 to 3 follow it.
	assert_eq!(t.scrollback.len(), 1);
	assert_eq!(t.prompt_click(0, 0), Some(click(1, 2)));
	assert_eq!(t.prompt_click(3, 0), Some(click(1, 5)));
}

#[test]
fn evicted_history_and_marks_do_not_move_the_prompt_line() {
	let mut t = terminal();
	t.set_scrollback_limit(1);
	send(
		&mut t,
		"\x1b]133;A;click_events=2\x07$ \x1b]133;B\x07a\r\nb\x1b]1337;SetMark\x07\r\nc\r\nd\r\ne\r\nf",
	);
	// Two lines scrolled out, one of them evicted: the screen's top row is
	// the third line from the prompt, and the mark is not a prompt.
	assert_eq!(t.scrollback.len(), 1);
	assert_eq!(t.prompt_click(0, 0), Some(click(1, 3)));
	assert_eq!(t.prompt_click(3, 0), Some(click(1, 6)));
}
