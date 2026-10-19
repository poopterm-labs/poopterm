//! ConEmu's OSC 9 commands and xterm's title and icon name (OSC 0, 1, 2).
use crate::{
	parser::Parser,
	shell_integration::Phase,
	term::Term,
	theme::{Theme, ThemeKind},
};
use std::path::Path;

fn terminal() -> Term {
	Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

#[test]
fn icon_names_and_titles_are_separate() {
	let mut t = terminal();
	send(&mut t, "\x1b]1;icon\x07");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("Shell", "icon"));
	assert_eq!(t.title_generation, 0);
	send(&mut t, "\x1b]2;window\x07");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("window", "icon"));
	send(&mut t, "\x1b]0;both\x07");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("both", "both"));
	// Icon names are bounded and cleaned like titles.
	send(&mut t, &format!("\x1b]1;a\u{85}{}\x07", "b".repeat(100)));
	assert_eq!(t.icon_name, format!("a{}", "b".repeat(63)));
}

#[test]
fn conemu_sets_the_working_directory() {
	let mut t = terminal();
	let generation = t.cwd_generation;
	send(&mut t, "\x1b]9;9;\"/srv/my dir\"\x07");
	assert_eq!(t.cwd, Path::new("/srv/my dir"));
	assert_eq!(t.cwd_generation, generation.wrapping_add(1));
	send(&mut t, "\x1b]9;9;/unquoted\x1b\\");
	assert_eq!(t.cwd, Path::new("/unquoted"));
	// Windows and relative paths, and control characters, are ignored.
	for path in ["\"C:\\\\Users\"", "relative", "\"/a\u{85}b\"", "\"\""] {
		send(&mut t, &format!("\x1b]9;9;{path}\x07"));
		assert_eq!(t.cwd, Path::new("/unquoted"), "{path}");
	}
}

#[test]
fn conemu_marks_prompts_where_the_cursor_is() {
	let mut t = terminal();
	send(&mut t, "ab\x1b]9;12\x07");
	assert_eq!(t.shell.phase, Phase::Prompt);
	// Unlike OSC 133;A there is no fresh line.
	assert_eq!((t.cx, t.cy), (2, 0));
	assert_eq!(t.shell.previous_prompt(1), Some(0));
}

#[test]
fn conemu_names_the_tab() {
	let mut t = terminal();
	send(&mut t, "\x1b]9;3;\"build\"\x07");
	assert_eq!(t.tab_title.as_deref(), Some("build"));
	assert_eq!(t.title, "Shell");
	// The tab redraws when its name changes, and only then.
	assert_eq!(t.title_generation, 1);
	send(&mut t, "\x1b]9;3;build\x07");
	assert_eq!(t.title_generation, 1);
	send(&mut t, "\x1b]9;3;logs;tail\x07");
	assert_eq!(t.tab_title.as_deref(), Some("logs;tail"));
	// An empty name gives the tab its usual title back.
	send(&mut t, "\x1b]9;3;\x07");
	assert_eq!(t.tab_title, None);
	send(&mut t, "\x1b]9;3;x\x07\x1b]9;3\x07");
	assert_eq!(t.tab_title, None);
}

#[test]
fn other_conemu_commands_do_nothing() {
	let mut t = terminal();
	for command in [
		"1;1000",
		"2;\"hi\"",
		"5",
		"6;\"Close(2)\"",
		"7;\"calc.exe\"",
		"8;\"HOME\"",
		"10",
		"10;3",
		"11;\"comment\"",
		"99;x",
	] {
		send(&mut t, &format!("\x1b]9;{command}\x07"));
	}
	assert!(t.responses.is_empty());
	assert!(t.notifications.requests.is_empty());
	assert_eq!((t.cx, t.cy), (0, 0));
	assert_eq!(t.tab_title, None);
}
