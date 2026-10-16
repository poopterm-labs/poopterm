//! Small, flat configuration format. Unknown keys and comments survive saves.
use std::{fs, io, path::Path};

#[derive(Clone, Copy)]
pub struct Preferences {
	pub font_size: usize,
	pub scrollback_lines: usize,
	pub restore_sessions: bool,
}

impl Default for Preferences {
	fn default() -> Self {
		Self {
			font_size: 18,
			scrollback_lines: 5000,
			restore_sessions: true,
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
		}
	}
	pub fn load() -> Self {
		Self::parse(&fs::read_to_string(super::config_path()).unwrap_or_default())
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
				let end = quoted.find('"')?;
				Some(quoted[..end].to_owned())
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
			"font_size = 999\nscrollback_lines = 999999999\nrestore_sessions = nonsense",
		);
		assert_eq!(p.font_size, 32);
		assert_eq!(p.scrollback_lines, 100_000);
		assert!(p.restore_sessions);
		assert_eq!(Preferences::parse("font_size = -1").font_size, 18);
	}
}
