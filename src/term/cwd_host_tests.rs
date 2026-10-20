//! OSC 7 from other hosts: their paths are not this machine's directories.
use std::path::PathBuf;

use crate::{
	parser::Parser,
	term::{Term, iterm::hostname},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/start".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn cwd(term: &mut Term, uri: &str) -> PathBuf {
	send(term, &format!("\x1b]7;{uri}\x07"));
	term.cwd.clone()
}

/// A host name that is not this machine's.
fn other_host() -> &'static str {
	if hostname().eq_ignore_ascii_case("web1") {
		"web2"
	} else {
		"web1"
	}
}

#[test]
fn every_name_for_this_machine_sets_the_directory() {
	let mut t = terminal();
	let local = hostname();
	let upper = local.to_ascii_uppercase();
	let short = local.split('.').next().unwrap();
	for (uri, path) in [
		("file:///a".to_owned(), "/a"),
		("file://localhost/b".to_owned(), "/b"),
		("file://LocalHost/c".to_owned(), "/c"),
		(format!("file://{local}/d"), "/d"),
		(format!("file://{upper}/e"), "/e"),
		(format!("file://{short}.lan/f"), "/f"),
	] {
		assert_eq!(cwd(&mut t, &uri), PathBuf::from(path), "{uri}");
	}
	assert_eq!(t.iterm.remote_label(local), None);
}

#[test]
fn another_host_names_the_remote_and_keeps_the_local_directory() {
	let mut t = terminal();
	let generation = t.cwd_generation;
	let remote = other_host();
	assert_eq!(
		cwd(&mut t, &format!("file://{remote}/srv")),
		PathBuf::from("/start")
	);
	assert_eq!(t.cwd_generation, generation, "nothing to save or reopen");
	assert_eq!(t.iterm.remote_label(hostname()), Some(remote));
	assert_eq!(cwd(&mut t, "file:///back"), PathBuf::from("/back"));
	assert_eq!(t.iterm.remote_label(hostname()), None, "back home");
}

#[test]
fn a_remote_directory_keeps_the_user_remote_host_gave() {
	let mut t = terminal();
	let remote = other_host();
	send(&mut t, &format!("\x1b]1337;RemoteHost=me@{remote}\x07"));
	cwd(
		&mut t,
		&format!("file://{}/srv", remote.to_ascii_uppercase()),
	);
	assert_eq!(
		t.iterm.remote_host.as_deref(),
		Some(&*format!("me@{remote}"))
	);
	cwd(&mut t, "file://elsewhere.example/srv");
	assert_eq!(t.iterm.remote_host.as_deref(), Some("elsewhere.example"));
}

#[test]
fn hosts_with_control_characters_or_overlong_ones_are_ignored() {
	let mut t = terminal();
	let long = "h".repeat(256);
	for uri in [format!("file://{long}/x"), "file://a\u{7f}b/x".to_owned()] {
		assert_eq!(cwd(&mut t, &uri), PathBuf::from("/start"), "{uri:?}");
		assert_eq!(t.iterm.remote_host, None, "{uri:?}");
	}
	let longest = "h".repeat(255);
	cwd(&mut t, &format!("file://{longest}/x"));
	assert_eq!(t.iterm.remote_host.as_deref(), Some(&*longest));
}

#[test]
fn host_names_compare_by_their_first_label() {
	use crate::term::iterm::{host_of, is_local, same_host};
	assert!(same_host("Zeta", "zeta.example.com"));
	assert!(!same_host("zeta", "zetab"), "a prefix is another host");
	assert!(!same_host("zetab", "zeta"));
	assert!(!same_host("zeta.example.com", "beta.example.com"));
	assert!(is_local("", "zeta") && is_local("LOCALHOST", "zeta"));
	assert!(!is_local("localhost.evil", "zeta"));
	assert_eq!(host_of("me@zeta"), "zeta");
	assert_eq!(host_of("a@b@zeta"), "zeta", "users may hold an @");
	assert_eq!(host_of("zeta"), "zeta");
}

#[test]
fn uris_without_a_path_or_file_scheme_change_nothing() {
	let mut t = terminal();
	for uri in ["file://web1", "http://localhost/x", "/plain/path", "file:"] {
		assert_eq!(cwd(&mut t, uri), PathBuf::from("/start"), "{uri}");
		assert_eq!(t.iterm.remote_host, None, "{uri}");
	}
	Parser::new().advance(&mut t, b"\x1b]7;file://loc\xffalhost/x\x07");
	assert_eq!(t.cwd, PathBuf::from("/start"), "not UTF-8");
	assert_eq!(t.iterm.remote_host, None, "not UTF-8");
	// Nor do they end a remote session.
	cwd(&mut t, &format!("file://{}/srv", other_host()));
	cwd(&mut t, "file://localhost");
	assert_eq!(t.iterm.remote_label(hostname()), Some(other_host()));
}

#[test]
fn vscode_directories_on_another_host_name_it_too() {
	let mut t = terminal();
	let remote = other_host();
	send(&mut t, &format!("\x1b]633;P;Cwd=file://{remote}/srv\x07"));
	assert_eq!(t.cwd, PathBuf::from("/start"));
	assert_eq!(t.iterm.remote_label(hostname()), Some(remote));
}
