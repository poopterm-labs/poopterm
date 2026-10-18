//! Poopterm's terminfo entry: locating it, compiling a private copy, and
//! choosing `TERM` for spawned shells.
//!
//! When the entry is not installed system-wide, poopterm compiles the
//! bundled source with `tic` into its data directory once and points
//! `TERMINFO_DIRS` at it, so `TERM=xterm-poopterm` works without setup.
//! Without `tic`, shells get `TERM=xterm-256color`.
use std::{
	env,
	ffi::OsString,
	fs,
	path::{Path, PathBuf},
	process::Command,
};

/// The entry's primary name; `poopterm` is an alias.
pub(crate) const NAME: &str = "xterm-poopterm";
/// The terminfo source, also printed by `poopterm --print-terminfo`.
pub(crate) const SOURCE: &str = include_str!("../terminfo/poopterm.terminfo");

/// `TERM` and terminfo search path for spawned shells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TermEnv {
	pub term: String,
	/// A value for `TERMINFO_DIRS` when a private copy is in use.
	pub terminfo_dirs: Option<OsString>,
	/// Further variables, e.g. for automatic shell integration.
	pub extra: Vec<(String, OsString)>,
}

/// Picks `TERM`: an explicit config value wins; `auto` (or none) means
/// [`NAME`] when its terminfo is available.
pub(crate) fn choose_term(configured: Option<&str>, available: bool) -> &str {
	match configured.map(str::trim) {
		Some(term) if !term.is_empty() && term != "auto" => term,
		_ if available => NAME,
		_ => "xterm-256color",
	}
}

/// Resolves the environment for spawned shells, compiling a private copy
/// of the entry if needed. `configured` is the `term` config key.
pub(crate) fn resolve(configured: Option<&str>) -> TermEnv {
	let explicit = configured.is_some_and(|term| !term.trim().is_empty() && term.trim() != "auto");
	if explicit || installed_in(&search_dirs()) {
		return TermEnv {
			term: choose_term(configured, true).to_owned(),
			terminfo_dirs: None,
			extra: Vec::new(),
		};
	}
	let private = private_dir();
	let compiled = private
		.as_deref()
		.is_some_and(|dir| is_current(dir) || compile_into(dir));
	let terminfo_dirs = compiled.then(|| {
		// A trailing empty entry keeps the system's default directories.
		let mut dirs = OsString::from(private.as_deref().unwrap_or(Path::new("")));
		dirs.push(":");
		if let Some(existing) = env::var_os("TERMINFO_DIRS") {
			dirs.push(existing);
		}
		dirs
	});
	TermEnv {
		term: choose_term(configured, compiled).to_owned(),
		terminfo_dirs,
		extra: Vec::new(),
	}
}

/// True if any directory holds the compiled entry, in the Linux (`x/`) or
/// macOS (`78/`, hex) layout.
pub(crate) fn installed_in(dirs: &[PathBuf]) -> bool {
	dirs
		.iter()
		.any(|dir| dir.join("x").join(NAME).is_file() || dir.join("78").join(NAME).is_file())
}

/// Directories ncurses searches, in its order.
fn search_dirs() -> Vec<PathBuf> {
	let mut dirs = Vec::new();
	if let Some(dir) = env::var_os("TERMINFO") {
		dirs.push(PathBuf::from(dir));
	}
	if let Some(home) = env::var_os("HOME") {
		dirs.push(PathBuf::from(home).join(".terminfo"));
	}
	if let Some(list) = env::var_os("TERMINFO_DIRS") {
		dirs.extend(env::split_paths(&list).filter(|dir| !dir.as_os_str().is_empty()));
	}
	dirs.extend(
		[
			"/etc/terminfo",
			"/lib/terminfo",
			"/usr/share/terminfo",
			"/usr/lib/terminfo",
			"/usr/local/share/terminfo",
			"/opt/homebrew/share/terminfo",
			"/usr/local/opt/ncurses/share/terminfo",
		]
		.map(PathBuf::from),
	);
	dirs
}

/// Where a private copy of the entry is compiled.
fn private_dir() -> Option<PathBuf> {
	crate::config::data_dir().map(|dir| dir.join("terminfo"))
}

/// Whether `dir` holds a copy compiled from the current [`SOURCE`].
fn is_current(dir: &Path) -> bool {
	installed_in(&[dir.to_path_buf()])
		&& fs::read_to_string(dir.join("source.stamp")).is_ok_and(|stamp| stamp == SOURCE)
}

/// Compiles [`SOURCE`] into `dir` with `tic -x`. Returns false if `tic` is
/// missing or fails.
pub(crate) fn compile_into(dir: &Path) -> bool {
	let source = dir.join("poopterm.terminfo");
	let compiled = fs::create_dir_all(dir).is_ok()
		&& fs::write(&source, SOURCE).is_ok()
		&& Command::new("tic")
			.arg("-x")
			.arg("-o")
			.arg(dir)
			.arg(&source)
			.output()
			.is_ok_and(|output| output.status.success());
	compiled && fs::write(dir.join("source.stamp"), SOURCE).is_ok()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::process::Command;

	struct TempDir(PathBuf);
	impl TempDir {
		fn new(name: &str) -> Self {
			let dir =
				std::env::temp_dir().join(format!("poopterm-terminfo-{}-{name}", std::process::id()));
			let _ = fs::remove_dir_all(&dir);
			fs::create_dir_all(&dir).unwrap();
			Self(dir)
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	fn have(tool: &str) -> bool {
		Command::new(tool).arg("-V").output().is_ok()
	}

	#[test]
	fn installed_entries_are_found_in_either_directory_layout() {
		let linux = TempDir::new("linux");
		fs::create_dir_all(linux.0.join("x")).unwrap();
		fs::write(linux.0.join("x").join(NAME), b"").unwrap();
		let mac = TempDir::new("mac");
		fs::create_dir_all(mac.0.join("78")).unwrap();
		fs::write(mac.0.join("78").join(NAME), b"").unwrap();
		let empty = TempDir::new("empty");
		assert!(installed_in(&[empty.0.clone(), linux.0.clone()]));
		assert!(installed_in(std::slice::from_ref(&mac.0)));
		assert!(!installed_in(std::slice::from_ref(&empty.0)));
	}

	#[test]
	fn term_falls_back_to_xterm_256color_without_the_entry() {
		assert_eq!(choose_term(None, true), NAME);
		assert_eq!(choose_term(None, false), "xterm-256color");
		assert_eq!(choose_term(Some("auto"), false), "xterm-256color");
		assert_eq!(
			choose_term(Some("screen-256color"), true),
			"screen-256color"
		);
	}

	#[test]
	fn a_private_copy_can_be_compiled() {
		if !have("tic") {
			return;
		}
		let dir = TempDir::new("compile");
		assert!(compile_into(&dir.0));
		assert!(installed_in(std::slice::from_ref(&dir.0)));
	}

	#[test]
	fn infocmp_escapes_are_decoded_in_every_notation() {
		// ncurses 6 prints ^? and \E; macOS's ncurses 5.7 prints octal.
		assert_eq!(decode("^?"), "\x7f");
		assert_eq!(decode("\\177"), "\x7f");
		assert_eq!(decode("\\E[A"), "\x1b[A");
		assert_eq!(decode("\\e[1;2D"), "\x1b[1;2D");
		assert_eq!(decode("^H\\054"), "\x08,");
	}

	/// Decodes an infocmp string value: `\E`/`\e`, octal `\NNN`, `^X`.
	fn decode(value: &str) -> String {
		let mut out = String::new();
		let mut chars = value.chars().peekable();
		while let Some(c) = chars.next() {
			match c {
				'\\' => match chars.next() {
					Some('E' | 'e') => out.push('\x1b'),
					Some(d @ '0'..='7') => {
						let mut code = d.to_digit(8).unwrap();
						for _ in 0..2 {
							if let Some(d) = chars.peek().and_then(|d| d.to_digit(8)) {
								code = code * 8 + d;
								chars.next();
							}
						}
						out.push(char::from(code as u8));
					}
					Some(other) => out.push(other),
					None => {}
				},
				'^' => match chars.next() {
					Some('?') => out.push('\x7f'),
					Some(c) => out.push(char::from(c as u8 & 0x1f)),
					None => out.push('^'),
				},
				c => out.push(c),
			}
		}
		out
	}

	/// Reads string capabilities from the compiled entry via infocmp.
	fn capabilities(dir: &Path) -> std::collections::HashMap<String, String> {
		let output = Command::new("infocmp")
			.args(["-x", "-1", "-A"])
			.arg(dir)
			.arg(NAME)
			.output()
			.unwrap();
		String::from_utf8_lossy(&output.stdout)
			.lines()
			.filter_map(|line| line.trim().trim_end_matches(',').split_once('='))
			.map(|(name, value)| (name.to_owned(), decode(value)))
			.collect()
	}

	#[test]
	fn advertised_keys_match_what_poopterm_sends() {
		if !have("tic") || !have("infocmp") {
			return;
		}
		use crate::input::keys::{KeyModes, Modifiers, encode_key};
		use winit::keyboard::{Key, NamedKey};
		let dir = TempDir::new("keys");
		assert!(compile_into(&dir.0));
		let caps = capabilities(&dir.0);
		// ncurses enables application cursor mode (smkx) before reading keys.
		let app = KeyModes {
			application_cursor: true,
			kitty: false,
		};
		let shift = Modifiers {
			shift: true,
			..Default::default()
		};
		let ctrl = Modifiers {
			ctrl: true,
			..Default::default()
		};
		let none = Modifiers::default();
		for (cap, key, mods) in [
			("kcuu1", NamedKey::ArrowUp, none),
			("kcud1", NamedKey::ArrowDown, none),
			("kcub1", NamedKey::ArrowLeft, none),
			("kcuf1", NamedKey::ArrowRight, none),
			("khome", NamedKey::Home, none),
			("kend", NamedKey::End, none),
			("kdch1", NamedKey::Delete, none),
			("kich1", NamedKey::Insert, none),
			("kpp", NamedKey::PageUp, none),
			("knp", NamedKey::PageDown, none),
			("kbs", NamedKey::Backspace, none),
			("kf1", NamedKey::F1, none),
			("kf5", NamedKey::F5, none),
			("kf12", NamedKey::F12, none),
			("kLFT", NamedKey::ArrowLeft, shift),
			("kLFT5", NamedKey::ArrowLeft, ctrl),
			("kf13", NamedKey::F1, shift),
			("kcbt", NamedKey::Tab, shift),
		] {
			let sent = encode_key(&Key::Named(key), mods, app).unwrap();
			assert_eq!(
				caps.get(cap).map(String::as_bytes),
				Some(sent.as_slice()),
				"{cap}"
			);
		}
	}

	#[test]
	fn sequences_generated_from_the_entry_take_effect() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::{CursorShape, Term},
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("tput");
		assert!(compile_into(&dir.0));
		let tput = |args: &[&str]| {
			Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.args(args)
				.output()
				.unwrap()
				.stdout
		};
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		for bytes in [
			tput(&["smcup"]),
			tput(&["setrgbf", "1", "2", "3"]),
			b"x".to_vec(),
			tput(&["rep", "65", "3"]),
			tput(&["Ss", "6"]),
		] {
			parser.advance(&mut term, &bytes);
		}
		assert!(
			term.primary_screen.is_some(),
			"smcup enters the alternate screen"
		);
		assert_eq!(term.fg, 0x010203, "setrgbf sets a truecolor foreground");
		let row: String = term.grid[0].iter().map(|c| c.ch).collect();
		assert_eq!(&row[..4], "xAAA", "rep repeats a character");
		assert_eq!(
			term.cursor_shape,
			CursorShape::Bar,
			"Ss sets the cursor shape"
		);
	}
}
