//! Small, flat configuration format. Unknown keys and comments survive saves.
use std::{
	env,
	ffi::OsString,
	fs, io,
	path::{Path, PathBuf},
};

use crate::theme::ThemeKind;

#[derive(Clone, Copy)]
pub struct Preferences {
	pub font_size: usize,
	pub scrollback_lines: usize,
	pub restore_sessions: bool,
	pub opacity: usize,
	pub show_tabs: bool,
	pub vertical_tabs: bool,
	pub show_new_tab_button: bool,
	pub show_settings_button: bool,
	pub show_bottom_bar: bool,
}

impl Default for Preferences {
	fn default() -> Self {
		Self {
			font_size: 18,
			scrollback_lines: 5000,
			restore_sessions: true,
			opacity: 100,
			show_tabs: true,
			vertical_tabs: false,
			show_new_tab_button: true,
			show_settings_button: true,
			show_bottom_bar: true,
		}
	}
}

impl Preferences {
	pub fn parse(text: &str) -> Self {
		let defaults = Self::default();
		Self {
			font_size: value(text, "font_size")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.font_size)
				.clamp(10, 32),
			scrollback_lines: value(text, "scrollback_lines")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.scrollback_lines)
				.min(100_000),
			restore_sessions: value(text, "restore_sessions")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.restore_sessions),
			opacity: value(text, "opacity")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.opacity)
				.clamp(60, 100),
			show_tabs: value(text, "show_tabs")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.show_tabs),
			vertical_tabs: value(text, "vertical_tabs")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.vertical_tabs),
			show_new_tab_button: value(text, "show_new_tab_button")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.show_new_tab_button),
			show_settings_button: value(text, "show_settings_button")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.show_settings_button),
			show_bottom_bar: value(text, "show_bottom_bar")
				.and_then(|s| s.parse().ok())
				.unwrap_or(defaults.show_bottom_bar),
		}
	}
	pub fn load() -> Self {
		Self::parse(&fs::read_to_string(config_path()).unwrap_or_default())
	}
}

pub fn value(text: &str, key: &str) -> Option<String> {
	text
		.lines()
		.filter_map(|line| {
			let (name, raw) = line.split_once('=')?;
			if name.trim() != key {
				return None;
			}
			let raw = raw.trim();
			if let Some(quoted) = raw.strip_prefix('"') {
				let mut value = String::new();
				let mut chars = quoted.chars();
				while let Some(ch) = chars.next() {
					match ch {
						'"' => return Some(value),
						'\\' => match chars.next()? {
							'"' => value.push('"'),
							'\\' => value.push('\\'),
							'n' => value.push('\n'),
							'r' => value.push('\r'),
							't' => value.push('\t'),
							other => {
								value.push('\\');
								value.push(other);
							}
						},
						other => value.push(other),
					}
				}
				None
			} else {
				Some(raw.split('#').next()?.trim().to_owned())
			}
		})
		.next_back()
}

pub fn updated(text: &str, values: &[(&str, String)]) -> String {
	let mut lines: Vec<String> = text
		.lines()
		.filter(|line| {
			line
				.split_once('=')
				.is_none_or(|(key, _)| !values.iter().any(|(name, _)| key.trim() == *name))
		})
		.map(str::to_owned)
		.collect();
	for (key, value) in values {
		lines.push(format!("{key} = {value}"));
	}
	format!("{}\n", lines.join("\n"))
}

pub fn save(path: &Path, values: &[(&str, String)]) -> io::Result<()> {
	if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
		fs::create_dir_all(parent)?;
	}
	let previous = match fs::read_to_string(path) {
		Ok(text) => text,
		Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
		Err(error) => return Err(error),
	};
	let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
	fs::write(&temporary, updated(&previous, values))?;
	fs::rename(&temporary, path)
}

/// Config file location: `$POOPTERM_CONFIG`, then `$XDG_CONFIG_HOME/poopterm`,
/// then the platform default.
pub fn config_path() -> PathBuf {
	if let Some(path) = env::var_os("POOPTERM_CONFIG") {
		return PathBuf::from(path);
	}
	user_dir("XDG_CONFIG_HOME", ".config")
		.unwrap_or_else(|| PathBuf::from("./poopterm"))
		.join("config.toml")
}

/// Per-user data directory for generated files: `$XDG_DATA_HOME/poopterm`
/// (default `~/.local/share/poopterm`), or `~/Library/Application Support/poopterm` on macOS.
pub fn data_dir() -> Option<PathBuf> {
	user_dir("XDG_DATA_HOME", ".local/share")
}

/// Per-user state that is not worth backing up, such as open tabs:
/// `$XDG_STATE_HOME/poopterm` (default `~/.local/state/poopterm`), or
/// `~/Library/Application Support/poopterm` on macOS.
pub fn state_dir() -> Option<PathBuf> {
	user_dir("XDG_STATE_HOME", ".local/state")
}

/// Regenerable files such as the font database:
/// `$XDG_CACHE_HOME/poopterm` (default `~/.cache/poopterm`), or
/// `~/Library/Application Support/poopterm` on macOS.
pub fn cache_dir() -> Option<PathBuf> {
	user_dir("XDG_CACHE_HOME", ".cache")
}

/// `poopterm` inside an XDG base directory. On macOS, an unset variable
/// means Application Support instead of `default`.
fn user_dir(variable: &str, default: &str) -> Option<PathBuf> {
	let home = env::var_os("HOME").map(PathBuf::from);
	let default = if cfg!(target_os = "macos") {
		"Library/Application Support"
	} else {
		default
	};
	Some(xdg_base(env::var_os(variable), home.as_deref(), default)?.join("poopterm"))
}

/// An XDG base directory: `value` if absolute (the spec says to ignore empty
/// and relative values), else `default` under `home`.
pub(crate) fn xdg_base(
	value: Option<OsString>,
	home: Option<&Path>,
	default: &str,
) -> Option<PathBuf> {
	value
		.map(PathBuf::from)
		.filter(|path| path.is_absolute())
		.or_else(|| Some(home?.join(default)))
}

pub fn read_config_value(key: &str) -> Option<String> {
	value(&fs::read_to_string(config_path()).ok()?, key)
}

pub fn read_config_theme() -> Option<ThemeKind> {
	ThemeKind::parse(&read_config_value("theme")?)
}

pub fn save_config_theme(kind: ThemeKind) -> io::Result<()> {
	save(&config_path(), &[("theme", format!("\"{}\"", kind.name()))])
}

pub fn quote_config_string(value: &str) -> String {
	let escaped = value
		.replace('\\', "\\\\")
		.replace('"', "\\\"")
		.replace('\n', "\\n")
		.replace('\r', "\\r")
		.replace('\t', "\\t");
	format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn comments_quotes_duplicates_and_unknown_keys() {
		let text = "# User config\nfont_size = 12\nfont_size = 20 # comment\ncustom = \"a#b\"\n";
		assert_eq!(value(text, "custom").as_deref(), Some("a#b"));
		assert_eq!(Preferences::parse(text).font_size, 20);
		let result = updated(text, &[("font_size", "24".into())]);
		assert_eq!(result.matches("font_size").count(), 1);
		assert!(result.contains("custom = \"a#b\""));
		assert!(result.starts_with("# User config\n"));
		assert_eq!(updated(&result, &[("font_size", "24".into())]), result);
	}
	#[test]
	fn invalid_preferences_are_bounded() {
		let p = Preferences::parse(
			"font_size = 999\nscrollback_lines = 999999999\nrestore_sessions = nonsense\nopacity = 120",
		);
		assert_eq!(p.font_size, 32);
		assert_eq!(p.scrollback_lines, 100_000);
		assert!(p.restore_sessions);
		assert_eq!(p.opacity, 100);
		assert_eq!(Preferences::parse("font_size = -1").font_size, 18);
		assert_eq!(Preferences::parse("opacity = 25").opacity, 60);
	}
	#[test]
	fn tab_and_bar_preferences_default_on_and_parse_independently() {
		let defaults = Preferences::default();
		assert!(defaults.show_tabs);
		assert!(!defaults.vertical_tabs);
		assert!(defaults.show_new_tab_button);
		assert!(defaults.show_settings_button);
		assert!(defaults.show_bottom_bar);

		let configured = Preferences::parse(
			"show_tabs = false\nvertical_tabs = true\nshow_new_tab_button = false\nshow_settings_button = false\nshow_bottom_bar = false",
		);
		assert!(!configured.show_tabs);
		assert!(configured.vertical_tabs);
		assert!(!configured.show_new_tab_button);
		assert!(!configured.show_settings_button);
		assert!(!configured.show_bottom_bar);
	}
	#[test]
	fn xdg_variables_must_be_absolute_paths() {
		// The XDG Base Directory spec says to ignore empty and relative values.
		let home = Path::new("/home/me");
		let base = |value: Option<&str>| xdg_base(value.map(Into::into), Some(home), ".local/state");
		assert_eq!(base(Some("/srv/state")), Some("/srv/state".into()));
		assert_eq!(base(Some("")), Some("/home/me/.local/state".into()));
		assert_eq!(base(Some("state")), Some("/home/me/.local/state".into()));
		assert_eq!(base(None), Some("/home/me/.local/state".into()));
		assert_eq!(xdg_base(None, None, ".local/state"), None);
	}
}
