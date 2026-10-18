//! Saved tab working directories (`sessions.txt` next to the config file).
use std::{env, fs, path::PathBuf};

use crate::config::{Preferences, config_path};

// Session state is separate from user settings; hex encoding preserves unusual paths.
pub(crate) fn read_session_dirs() -> Vec<PathBuf> {
	fs::read_to_string(config_path().with_file_name("sessions.txt"))
		.unwrap_or_default()
		.lines()
		.filter_map(decode_session_path)
		.take(12)
		.collect()
}

pub(crate) fn decode_session_path(line: &str) -> Option<PathBuf> {
	if !line.len().is_multiple_of(2) {
		return None;
	}
	let bytes = line
		.as_bytes()
		.as_chunks::<2>()
		.0
		.iter()
		.map(|pair| {
			let text = std::str::from_utf8(pair).ok()?;
			u8::from_str_radix(text, 16).ok()
		})
		.collect::<Option<Vec<_>>>()?;
	String::from_utf8(bytes).ok().map(PathBuf::from)
}

pub(crate) fn save_session_dirs(dirs: &[PathBuf]) -> std::io::Result<()> {
	let path = config_path().with_file_name("sessions.txt");
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let text = dirs
		.iter()
		.take(12)
		.map(|dir| {
			dir
				.to_string_lossy()
				.as_bytes()
				.iter()
				.map(|b| format!("{b:02x}"))
				.collect::<String>()
		})
		.collect::<Vec<_>>()
		.join("\n");
	let temp = path.with_extension(format!("{}.tmp", std::process::id()));
	fs::write(&temp, text)?;
	fs::rename(temp, path)
}

pub(crate) fn default_cwd() -> PathBuf {
	env::var_os("HOME")
		.map(PathBuf::from)
		.or_else(|| env::current_dir().ok())
		.unwrap_or_else(|| PathBuf::from("/"))
}

pub(crate) fn restored_session_dirs() -> Vec<PathBuf> {
	if !Preferences::load().restore_sessions {
		return vec![default_cwd()];
	}
	let dirs: Vec<_> = read_session_dirs()
		.into_iter()
		.map(|dir| if dir.is_dir() { dir } else { default_cwd() })
		.collect();
	if dirs.is_empty() {
		vec![default_cwd()]
	} else {
		dirs
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn session_paths_preserve_punctuation() {
		let path = "/tmp/a,b#c \"quoted\"/猫";
		let encoded = path
			.as_bytes()
			.iter()
			.map(|b| format!("{b:02x}"))
			.collect::<String>();
		assert_eq!(decode_session_path(&encoded), Some(path.into()));
		assert!(decode_session_path("xyz").is_none());
	}
}
