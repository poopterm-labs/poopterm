//! Shell integration: prompt/command markers (OSC 133), working-directory
//! reports (OSC 7), and the optional shell hooks that emit them.
pub(crate) mod command_line;
pub(crate) mod inject;
mod markers;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

pub(crate) use markers::{Phase, ShellIntegration};

pub(crate) fn decode_uri_path(path: &str) -> Option<PathBuf> {
	let mut decoded = Vec::with_capacity(path.len());
	let mut bytes = path.bytes();
	while let Some(byte) = bytes.next() {
		let byte = if byte == b'%' {
			let high = (bytes.next()? as char).to_digit(16)?;
			let low = (bytes.next()? as char).to_digit(16)?;
			(high * 16 + low) as u8
		} else {
			byte
		};
		if byte == 0 || byte < 32 {
			return None;
		}
		decoded.push(byte);
	}
	let path = PathBuf::from(String::from_utf8(decoded).ok()?);
	path.is_absolute().then_some(path)
}

/// Returns the optional prompt hooks for `shell`, as printed by
/// `poopterm --print-shell-integration SHELL`.
pub(crate) fn script(shell: &str) -> Result<&'static str, String> {
	match shell {
		"zsh" => Ok(include_str!("../../shell-integration/zsh.zsh")),
		"bash" => Ok(include_str!("../../shell-integration/bash.sh")),
		"fish" => Ok(include_str!("../../shell-integration/fish.fish")),
		_ => Err(format!(
			"unsupported shell: {shell}; choose zsh, bash, or fish"
		)),
	}
}
