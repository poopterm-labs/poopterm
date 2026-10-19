//! Semantic prompts (OSC 133 with the FinalTerm, semantic-prompts and kitty
//! options), from bytes in to markers, cursor moves and click replies out.
use crate::{
	parser::Parser,
	shell_integration::Phase,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn phases(term: &Term) -> Vec<Phase> {
	term
		.shell
		.markers
		.iter()
		.map(|marker| marker.phase)
		.collect()
}

#[test]
fn fresh_lines_start_prompts_at_the_left_margin() {
	let mut t = terminal();
	send(&mut t, "ab\x1b]133;L\x07");
	assert_eq!((t.cx, t.cy), (0, 1));
	// Already at the margin: nothing moves.
	send(&mut t, "\x1b]133;L\x07");
	assert_eq!((t.cx, t.cy), (0, 1));
	// A full line waiting to wrap still needs a new line.
	send(&mut t, "0123456789\x1b]133;A\x07");
	assert_eq!((t.cx, t.cy), (0, 2));
	assert_eq!(t.shell.markers[0].row, 2);
	send(&mut t, "x\x1b]133;N;aid=1\x07");
	assert_eq!((t.cx, t.cy), (0, 3));
	// Other markers never move the cursor.
	send(&mut t, "$ \x1b]133;B\x07");
	assert_eq!((t.cx, t.cy), (2, 3));
	// One column: the cursor stays at the margin while the wrap waits.
	let mut narrow = Term::new(1, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	send(&mut narrow, "x\x1b]133;L\x07");
	assert_eq!((narrow.cx, narrow.cy, narrow.wrap_pending), (0, 1, false));
}

#[test]
fn options_after_the_marker_are_understood() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]133;A;aid=42;cl=line;redraw=0\x07$ \x1b]133;B;future\x07ls\x1b]133;C;aid=42\x07\
		 \x1b]133;D;0;aid=42\x07",
	);
	assert_eq!(
		phases(&t),
		[
			Phase::Prompt,
			Phase::Command,
			Phase::Output,
			Phase::Finished
		]
	);
	assert_eq!(t.shell.last_exit_code, Some(0));
	assert_eq!(t.shell.last_error, None);
	// I ends the prompt like B; unknown letters are ignored.
	send(&mut t, "\x1b]133;A\x07\x1b]133;I\x07\x1b]133;Q\x07");
	assert_eq!(t.shell.phase, Phase::Command);
	assert_eq!(t.shell.markers.len(), 6);
}

#[test]
fn errors_take_precedence_over_exit_codes() {
	let mut t = terminal();
	let cases = [
		("D;1;err=", Some(0), None),
		("D;0;err=2", Some(2), Some("2")),
		("D;0;err=0", Some(0), Some("0")),
		("D;;err=CANCEL", None, Some("CANCEL")),
		("D;3", Some(3), None),
		("D", None, None),
	];
	for (marker, code, error) in cases {
		send(&mut t, &format!("\x1b]133;C\x07\x1b]133;{marker}\x07"));
		assert_eq!(t.shell.last_exit_code, code, "{marker}");
		assert_eq!(t.shell.last_error.as_deref(), error, "{marker}");
	}
	// Error text is bounded and printable.
	let long = "e".repeat(100);
	send(&mut t, &format!("\x1b]133;D;1;err={long}\x07"));
	assert_eq!(t.shell.last_error.as_deref().map(str::len), Some(32));
}

#[test]
fn secondary_prompts_are_not_commands() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]133;A\x07$ \x1b]133;B\x07for x\r\n\x1b]133;A;k=s\x07> \x1b]133;B\x07\
		 do\r\n\x1b]133;P;k=c\x07> \x1b]133;B\x07",
	);
	assert_eq!(t.shell.previous_prompt(3), Some(0));
	assert_eq!(t.shell.phase, Phase::Command);
}

#[test]
fn new_commands_finish_the_open_one_with_their_application() {
	let mut t = terminal();
	let finished = |t: &Term| phases(t).iter().filter(|&&p| p == Phase::Finished).count();
	// A shell runs python, whose own prompts nest inside the output.
	send(
		&mut t,
		"\x1b]133;A;aid=sh\x07\x1b]133;B\x07\x1b]133;C\x07\
		 \x1b]133;N;aid=py\x07\x1b]133;B\x07\x1b]133;C\x07",
	);
	assert_eq!(finished(&t), 0);
	// The next python prompt finishes python's command only.
	send(&mut t, "\x1b]133;N;aid=py\x07");
	assert_eq!(finished(&t), 1);
	assert_eq!(t.shell.phase, Phase::Prompt);
	// The shell's prompt finishes its command and python's prompt too.
	send(&mut t, "\x1b]133;N;aid=sh\x07");
	assert_eq!(finished(&t), 2);
	// No aid matches the empty one: a plain N finishes a plain command.
	send(&mut t, "\x1b]133;A\x07\x1b]133;C\x07\x1b]133;N\x07");
	assert_eq!(finished(&t), 3);
	assert!(t.shell.last_output().is_some());
	// A redrawn prompt replaces the one that never ran, so the shell's D
	// closes the shell's command and nothing is left open.
	send(
		&mut t,
		"\x1b]133;A;aid=sh\x07\x1b]133;C\x07\x1b]133;A;aid=py\x07\x1b]133;A;aid=py\x07\
		 \x1b]133;C\x07\x1b]133;D\x07\x1b]133;D\x07\x1b]133;N;aid=sh\x07",
	);
	assert_eq!(finished(&t), 5);
}

#[test]
fn command_lines_are_decoded() {
	let mut t = terminal();
	let cases = [
		(r"cmdline=echo\ hi\;\ there", "echo hi; there"),
		(r"cmdline=git commit -m 'a b' \$x", "git commit -m a b $x"),
		(r"cmdline=printf $'a\tb\x41\101é\n'", "printf a\tbAAé\n"),
		(r#"cmdline=echo "a \"b\" \c""#, r#"echo a "b" \c"#),
		("cmdline_url=ls%20-l%3B%C3%A9", "ls -l;é"),
	];
	for (option, expected) in cases {
		send(&mut t, &format!("\x1b]133;C;{option}\x07"));
		assert_eq!(t.shell.command_line.as_deref(), Some(expected), "{option}");
		send(&mut t, "\x1b]133;D;0\x07");
	}
	// Broken quoting, bad escapes and overlong lines are dropped.
	let long = format!("cmdline={}", "x".repeat(4097));
	for option in [
		"cmdline='open",
		"cmdline_url=%zz",
		"cmdline_url=%ff",
		long.as_str(),
	] {
		send(&mut t, &format!("\x1b]133;C;{option}\x07"));
		assert_eq!(t.shell.command_line, None, "{option}");
	}
	// The next prompt forgets the command.
	send(
		&mut t,
		"\x1b]133;C;cmdline=ls\x07\x1b]133;D\x07\x1b]133;A\x07",
	);
	assert_eq!(t.shell.command_line, None);
}

#[test]
fn clicks_in_the_input_line_move_the_cursor_when_asked() {
	let mut t = terminal();
	// "$ " prompt, then "a界bc" typed, cursor after "a".
	send(&mut t, "\x1b]133;A;cl=line\x07$ \x1b]133;B\x07a界bc\x1b[4D");
	assert_eq!(t.cx, 3);
	// Right two characters: the wide one counts once.
	assert_eq!(t.prompt_click(0, 6).as_deref(), Some(&b"\x1b[C\x1b[C"[..]));
	// Into the prompt: the input start; past the end: just after it.
	assert_eq!(t.prompt_click(0, 0).as_deref(), Some(&b"\x1b[D"[..]));
	assert_eq!(
		t.prompt_click(0, 9).as_deref(),
		Some(&b"\x1b[C\x1b[C\x1b[C"[..])
	);
	assert_eq!(t.prompt_click(0, 3).as_deref(), Some(&b""[..]));
	// The right half of a wide character is that character.
	assert_eq!(t.prompt_click(0, 4).as_deref(), Some(&b""[..]));
	// Cursor keys follow DECCKM.
	send(&mut t, "\x1b[?1h");
	assert_eq!(t.prompt_click(0, 6).as_deref(), Some(&b"\x1bOC\x1bOC"[..]));
	send(&mut t, "\x1b[?1l");
	// Other rows, other phases, and prompts without cl are left alone.
	assert_eq!(t.prompt_click(1, 3), None);
	send(&mut t, "\x1b]133;C\x07");
	assert_eq!(t.prompt_click(0, 6), None);
	send(&mut t, "\r\n\x1b]133;A\x07$ \x1b]133;B\x07ab");
	assert_eq!(t.prompt_click(1, 2), None);
}
