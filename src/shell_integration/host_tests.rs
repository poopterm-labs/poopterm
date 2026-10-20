//! The hooks' OSC 7 names this machine, so the directory counts as local.
use std::path::Path;

use super::tests::run;
use crate::{
	parser::Parser,
	term::{Term, iterm::hostname},
	theme::{Theme, ThemeKind},
};

/// Runs a prompt hook in /tmp and checks that its OSC 7 names this host and
/// that a terminal reading it moves to /tmp without naming a remote host.
fn reports_this_host(shell: &str, args: &[&str], script: &str) {
	let Some((stdout, stderr)) = run(shell, args, script) else {
		return;
	};
	assert_eq!(stderr, "");
	let expected = format!("\x1b]7;file://{}/tmp\x07", hostname());
	assert!(stdout.contains(&expected), "{stdout:?}");
	let mut term = Term::new(80, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, stdout.as_bytes());
	assert_eq!(term.cwd, Path::new("/tmp"));
	assert_eq!(term.iterm.remote_label(hostname()), None);
}

#[test]
fn zsh_names_this_host() {
	let script = "source shell-integration/zsh.zsh; cd /tmp; _poopterm_precmd";
	reports_this_host("zsh", &["-fc"], script);
}

#[test]
fn bash_names_this_host() {
	let script = "source shell-integration/bash.sh; cd /tmp; __poopterm_prompt_command";
	reports_this_host("bash", &["--norc", "-c"], script);
}

#[test]
fn fish_names_this_host() {
	let script = "source shell-integration/fish.fish; cd /tmp; __poopterm_prompt";
	reports_this_host("fish", &["--no-config", "-c"], script);
}
