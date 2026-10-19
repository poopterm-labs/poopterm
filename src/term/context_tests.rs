//! OSC 3008 hierarchical contexts: starting, updating and ending them, field
//! validation, limits, and the label the status bar shows.
use super::contexts::{Kind, MAX_DEPTH};
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

fn ids(term: &Term) -> Vec<&str> {
	term
		.contexts
		.stack()
		.iter()
		.map(|context| context.id.as_str())
		.collect()
}

#[test]
fn contexts_nest_update_and_end() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]3008;start=boot;type=shell;user=me;hostname=zeta;pid=42\x1b\\\
		 \x1b]3008;start=run0;type=elevate;targetuser=root\x1b\\\
		 \x1b]3008;start=cmd;type=command;cmdline=ls a\\x3bb\\x5c\x1b\\",
	);
	assert_eq!(ids(&t), ["boot", "run0", "cmd"]);
	let boot = &t.contexts.stack()[0];
	assert_eq!(boot.kind, Some(Kind::Shell));
	assert_eq!(
		(
			boot.field("user"),
			boot.field("hostname"),
			boot.field("pid")
		),
		(Some("me"), Some("zeta"), Some("42"))
	);
	assert_eq!(t.contexts.stack()[2].field("cmdline"), Some("ls a;b\\"));
	// Starting a known context again replaces its fields and ends what is
	// nested in it.
	send(&mut t, "\x1b]3008;start=run0;type=chpriv\x1b\\");
	assert_eq!(ids(&t), ["boot", "run0"]);
	assert_eq!(t.contexts.stack()[1].kind, Some(Kind::Chpriv));
	assert_eq!(t.contexts.stack()[1].field("targetuser"), None);
	// Ending a context ends its subcontexts; unknown ids change nothing.
	send(&mut t, "\x1b]3008;start=x\x1b\\\x1b]3008;end=nope\x1b\\");
	assert_eq!(ids(&t), ["boot", "run0", "x"]);
	send(&mut t, "\x1b]3008;end=run0;exit=success;status=0\x1b\\");
	assert_eq!(ids(&t), ["boot"]);
	// Resets leave contexts alone.
	send(&mut t, "\x1bc\x1b[!p");
	assert_eq!(ids(&t), ["boot"]);
}

#[test]
fn invalid_fields_are_dropped_and_invalid_ids_drop_the_sequence() {
	let mut t = terminal();
	let long = "u".repeat(256);
	send(
		&mut t,
		&format!(
			"\x1b]3008;start=a;type=spaceship;user={long};pid=4x;machineid=xyz;\
			 bootid=d4a3d0fdf2e24fdea6d971ce73f4fbf2;comm=;cwd=a\\qb;cmdline=;future=1;hostname=h\x1b\\"
		),
	);
	let context = &t.contexts.stack()[0];
	assert_eq!(context.kind, None);
	for name in ["user", "pid", "machineid", "comm", "cwd", "future"] {
		assert_eq!(context.field(name), None, "{name}");
	}
	assert_eq!(
		context.field("bootid"),
		Some("d4a3d0fdf2e24fdea6d971ce73f4fbf2")
	);
	assert_eq!(context.field("cmdline"), Some(""));
	assert_eq!(context.field("hostname"), Some("h"));
	// UTF-8 is welcome, encoded C1 controls are not.
	send(
		&mut t,
		"\x1b]3008;start=a;user=j\u{f6}rg;comm=a\u{85}b\x1b\\",
	);
	let context = &t.contexts.stack()[0];
	assert_eq!(
		(context.field("user"), context.field("comm")),
		(Some("j\u{f6}rg"), None)
	);
	let long_id = "i".repeat(65);
	for id in ["", long_id.as_str(), "a\\x7", "a\\b"] {
		send(&mut t, &format!("\x1b]3008;start={id};type=shell\x1b\\"));
		assert_eq!(ids(&t), ["a"], "{id}");
	}
	send(&mut t, "\x1b]3008;begin=b\x1b\\\x1b]3008\x1b\\");
	assert_eq!(ids(&t), ["a"]);
	// Escaped separators are allowed in ids too.
	send(&mut t, "\x1b]3008;start=b\\x3bc\\X5C\x1b\\");
	assert_eq!(ids(&t), ["a", "b;c\\"]);
}

#[test]
fn the_stack_keeps_the_earliest_contexts() {
	let mut t = terminal();
	for index in 0..MAX_DEPTH + 3 {
		send(&mut t, &format!("\x1b]3008;start=c{index}\x1b\\"));
	}
	let stack = ids(&t);
	assert_eq!(stack.len(), MAX_DEPTH);
	assert_eq!(
		stack.last().copied(),
		Some(format!("c{}", MAX_DEPTH - 1).as_str())
	);
}

#[test]
fn labels_name_privileges_and_places() {
	let mut t = terminal();
	assert_eq!(t.contexts.label(), None);
	let cases = [
		("type=shell;user=me", None),
		("type=elevate", Some("root")),
		("type=chpriv;targetuser=postgres", Some("postgres")),
		("type=remote;targetuser=me;targethost=web1", Some("me@web1")),
		("type=remote;targethost=web1", Some("web1")),
		("type=container;container=foobar", Some("foobar")),
		("type=vm;vm=win11", Some("win11")),
		// Commands inside keep the label of the place they run in.
		("type=command;cmdline=ls", Some("win11")),
	];
	for (index, (fields, label)) in cases.into_iter().enumerate() {
		send(&mut t, &format!("\x1b]3008;start=c{index};{fields}\x1b\\"));
		assert_eq!(t.contexts.label().as_deref(), label, "{fields}");
	}
}
