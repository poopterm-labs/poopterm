//! Shell integration beyond OSC 133: the working directory (OSC 7) and VS
//! Code's OSC 633 markers, command lines and properties.
use crate::{shell_integration::decode_uri_path, term::Term};

impl Term {
	/// Sets the working directory from a `file://` URI.
	pub(super) fn osc_cwd_uri(&mut self, uri: &[u8]) {
		if let Ok(uri) = std::str::from_utf8(uri)
			&& let Some(path) = uri.strip_prefix("file://")
			&& let Some(path) = path
				.find('/')
				.and_then(|index| decode_uri_path(&path[index..]))
			&& self.cwd != path
		{
			self.cwd = path;
			self.cwd_generation = self.cwd_generation.wrapping_add(1);
		}
	}

	/// OSC 633: A to D mark like OSC 133 (A without a fresh line), E sets
	/// the command line, and P sets properties, of which only `Cwd` is used.
	pub(super) fn osc_vscode(&mut self, params: &[&[u8]]) {
		match params.first().copied().unwrap_or_default() {
			b"A" | b"B" | b"C" | b"D" => {
				if params[0] == b"A" {
					self.status.prompt();
				}
				let mut marker: Vec<&[u8]> = vec![b"133"];
				marker.extend_from_slice(params);
				self.shell.record(
					&marker,
					self.history_base + self.scrollback.len() + self.cy,
					self.cx,
					std::time::Instant::now(),
				);
			}
			// The nonce proves the line came from VS Code's own scripts; we
			// inject none, so it is not checked.
			b"E" => {
				self.shell.command_line = params
					.get(1)
					.and_then(|line| std::str::from_utf8(line).ok())
					.and_then(unescape_command_line);
			}
			b"P" => {
				let Some((b"Cwd", value)) = params.get(1).and_then(|property| {
					let at = property.iter().position(|&byte| byte == b'=')?;
					Some((&property[..at], &property[at + 1..]))
				}) else {
					return;
				};
				if value.starts_with(b"file://") {
					self.osc_cwd_uri(value);
				} else if let Ok(path) = std::str::from_utf8(value)
					&& path.starts_with('/')
					&& !path.chars().any(char::is_control)
					&& self.cwd != std::path::Path::new(path)
				{
					self.cwd = path.into();
					self.cwd_generation = self.cwd_generation.wrapping_add(1);
				}
			}
			_ => {}
		}
	}
}

/// Undoes OSC 633 E escaping: `\\` and `\xAB`.
fn unescape_command_line(text: &str) -> Option<String> {
	use crate::shell_integration::command_line::MAX_COMMAND_LINE;
	let mut out = Vec::with_capacity(text.len());
	let mut bytes = text.bytes();
	while let Some(byte) = bytes.next() {
		if byte != b'\\' {
			out.push(byte);
			continue;
		}
		match bytes.next()? {
			b'\\' => out.push(b'\\'),
			b'x' => {
				let high = char::from(bytes.next()?).to_digit(16)?;
				let low = char::from(bytes.next()?).to_digit(16)?;
				out.push((high * 16 + low) as u8);
			}
			_ => return None,
		}
	}
	(out.len() <= MAX_COMMAND_LINE).then_some(())?;
	String::from_utf8(out).ok()
}
