//! OSC 133 semantic shell markers and OSC 7 directory URIs.
use std::{collections::VecDeque, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
	#[default]
	Unknown,
	Prompt,
	Command,
	Output,
	Finished,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
	pub phase: Phase,
	pub row: usize,
	pub col: usize,
	pub exit_code: Option<i32>,
}
#[derive(Clone, Default)]
pub(crate) struct ShellIntegration {
	pub phase: Phase,
	pub last_exit_code: Option<i32>,
	pub markers: VecDeque<Marker>,
}
impl ShellIntegration {
	pub fn record(&mut self, params: &[&[u8]], row: usize, col: usize) {
		let phase = match params.get(1).copied() {
			Some(b"A") => Phase::Prompt,
			Some(b"B") => Phase::Command,
			Some(b"C") => Phase::Output,
			Some(b"D") => Phase::Finished,
			_ => return,
		};
		let exit_code = if phase == Phase::Finished {
			params
				.get(2)
				.and_then(|p| std::str::from_utf8(p).ok())
				.and_then(|s| s.parse().ok())
		} else {
			None
		};
		if phase == Phase::Finished {
			self.last_exit_code = exit_code;
		}
		if phase == Phase::Output {
			self.last_exit_code = None;
		}
		self.phase = phase;
		self.markers.push_back(Marker {
			phase,
			row,
			col,
			exit_code,
		});
		if self.markers.len() > 256 {
			self.markers.pop_front();
		}
	}
}

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
		"zsh" => Ok(include_str!("../shell-integration/zsh.zsh")),
		"bash" => Ok(include_str!("../shell-integration/bash.sh")),
		"fish" => Ok(include_str!("../shell-integration/fish.fish")),
		_ => Err(format!(
			"unsupported shell: {shell}; choose zsh, bash, or fish"
		)),
	}
}

#[cfg(test)]
mod tests {
	use std::process::Command;

	/// Runs `script` in `shell`, or returns `None` when the shell is not installed.
	fn run(shell: &str, args: &[&str], script: &str) -> Option<(String, String)> {
		let output = Command::new(shell).args(args).arg(script).output().ok()?;
		Some((
			String::from_utf8_lossy(&output.stdout).into_owned(),
			String::from_utf8_lossy(&output.stderr).into_owned(),
		))
	}

	#[test]
	fn zsh_hooks_report_exit_status_and_directory_without_errors() {
		let script = concat!(
			"source shell-integration/zsh.zsh; cd /tmp; false; _poopterm_precmd; ",
			"print -rn -- \"[$PATH]\""
		);
		let Some((stdout, stderr)) = run("zsh", &["-fc"], script) else {
			return;
		};
		assert_eq!(stderr, "");
		assert!(stdout.contains("\x1b]133;D;1\x07"), "{stdout:?}");
		assert!(stdout.contains("\x1b]7;file:///tmp\x07"), "{stdout:?}");
		assert!(!stdout.contains("[/tmp]"), "hook must not clobber $PATH");
	}

	#[test]
	fn bash_marks_command_input_after_the_prompt() {
		let script = concat!(
			"PS1='P> '; source shell-integration/bash.sh; printf '%s|' \"$PS1\"; ",
			"false; __poopterm_prompt_command"
		);
		let Some((stdout, stderr)) = run("bash", &["--norc", "-c"], script) else {
			return;
		};
		assert_eq!(stderr, "");
		let (ps1, prompt) = stdout.split_once('|').unwrap();
		assert_eq!(ps1, "P> \\[\\e]133;B\\a\\]");
		assert!(prompt.starts_with("\x1b]133;D;1\x07"), "{prompt:?}");
	}

	/// A directory whose name contains BEL, ESC and other bytes outside the
	/// URI-safe set. Raw control bytes in OSC 7 would end the sequence early
	/// and let the rest of the name run as terminal commands.
	struct HostileDirectory {
		root: std::path::PathBuf,
		path: std::path::PathBuf,
	}

	impl HostileDirectory {
		fn new(shell: &str) -> Self {
			let root =
				std::env::temp_dir().join(format!("poopterm-hooks-{}-{shell}", std::process::id()));
			let path = root.join("a b\x07\x1b]52;c;eA==\x07é%");
			std::fs::create_dir_all(&path).unwrap();
			Self { root, path }
		}
	}

	impl Drop for HostileDirectory {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(&self.root);
		}
	}

	const ENCODED_TAIL: &str = "/a%20b%07%1B%5D52%3Bc%3BeA%3D%3D%07%C3%A9%25";

	#[test]
	fn zsh_percent_encodes_unsafe_directory_bytes() {
		let dir = HostileDirectory::new("zsh");
		let script = "source shell-integration/zsh.zsh; cd -- \"$TARGET\"; _poopterm_precmd";
		let Some(output) = Command::new("zsh")
			.args(["-fc", script])
			.env("TARGET", &dir.path)
			.output()
			.ok()
		else {
			return;
		};
		let stdout = String::from_utf8_lossy(&output.stdout);
		let osc7 = stdout.split("\x1b]7;").nth(1).unwrap_or_default();
		assert!(osc7.ends_with(&format!("{ENCODED_TAIL}\x07")), "{stdout:?}");
		assert_eq!(osc7.matches('\x07').count(), 1, "{stdout:?}");
	}

	#[test]
	fn bash_percent_encodes_unsafe_directory_bytes() {
		let dir = HostileDirectory::new("bash");
		let script = "source shell-integration/bash.sh; cd -- \"$TARGET\"; __poopterm_prompt_command";
		let Some(output) = Command::new("bash")
			.args(["--norc", "-c", script])
			.env("TARGET", &dir.path)
			.output()
			.ok()
		else {
			return;
		};
		let stdout = String::from_utf8_lossy(&output.stdout);
		let osc7 = stdout.split("\x1b]7;").nth(1).unwrap_or_default();
		assert!(osc7.ends_with(&format!("{ENCODED_TAIL}\x07")), "{stdout:?}");
		assert_eq!(osc7.matches('\x07').count(), 1, "{stdout:?}");
	}
}
