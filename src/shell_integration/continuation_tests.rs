//! Continuation prompts (PS2) are marked, so they are not command input.
use super::tests::run;
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

/// Feeds the prompts a shell rendered, around a command typed over two
/// lines, and returns where input starts on the second.
fn continued_input_start(rendered: &str) -> usize {
	// The shell prints `primary|secondary`, without its non-printing markers.
	let (primary, secondary) = rendered.split_once('|').unwrap();
	let screen = format!("{primary}for x\r\n{secondary}do");
	let mut term = Term::new(20, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, screen.as_bytes());
	term.shell.input_start(1)
}

#[test]
fn bash_marks_its_continuation_prompt() {
	let script = concat!(
		"PS1='$ '; PS2='> '; source shell-integration/bash.sh; ",
		"p=${PS1@P}; s=${PS2@P}; printf '%s|%s' \"$p\" \"$s\" | tr -d '\\001\\002'"
	);
	let Some((stdout, stderr)) = run("bash", &["--norc", "-c"], script) else {
		return;
	};
	assert_eq!(stderr, "");
	assert!(
		stdout.contains("\x1b]133;A;k=s\x07> \x1b]133;B\x07"),
		"{stdout:?}"
	);
	assert_eq!(continued_input_start(&stdout), 2, "after `> `");
}

#[test]
fn zsh_marks_its_continuation_prompt() {
	// zle-line-init sends B after either prompt; print it as zle would.
	let script = concat!(
		"PS1='$ '; PS2='> '; source shell-integration/zsh.zsh; ",
		"print -Pn -- \"$PS1\"; _poopterm_line_init; print -n '|'; ",
		"print -Pn -- \"$PS2\"; _poopterm_line_init"
	);
	let Some((stdout, stderr)) = run("zsh", &["-fc"], script) else {
		return;
	};
	assert_eq!(stderr, "");
	assert!(stdout.contains("\x1b]133;A;k=s\x07> "), "{stdout:?}");
	assert_eq!(continued_input_start(&stdout), 2, "after `> `");
}
