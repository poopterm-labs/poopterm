//! Shell integration beyond OSC 133: the working directory (OSC 7), ConEmu's
//! OSC 9 commands, and VS Code's OSC 633 markers, command lines and
//! properties.
use crate::{
	shell_integration::decode_uri_path,
	term::{
		Term,
		iterm::{host_of, hostname, is_local, same_host},
	},
};

impl Term {
	/// Sets the working directory from a `file://` URI. A directory on
	/// another host is not one here: it only names the remote host.
	pub(super) fn osc_cwd_uri(&mut self, uri: &[u8]) {
		let Some((host, path)) = std::str::from_utf8(uri)
			.ok()
			.and_then(|uri| uri.strip_prefix("file://"))
			.and_then(|rest| rest.find('/').map(|at| rest.split_at(at)))
		else {
			return;
		};
		if host.len() > 255 || host.chars().any(char::is_control) {
			return;
		}
		let local = hostname();
		if !is_local(host, local) {
			// RemoteHost may have named the user too; keep it.
			let known = self.iterm.remote_host.as_deref();
			if !known.is_some_and(|known| same_host(host_of(known), host)) {
				self.iterm.remote_host = Some(host.to_owned());
			}
			return;
		}
		if self.iterm.remote_label(local).is_some() {
			self.iterm.remote_host = None;
		}
		if let Some(path) = decode_uri_path(path)
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
			b"A" | b"B" | b"C" | b"D" => self.shell_marker(params),
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
				} else {
					self.set_cwd_path(value);
				}
			}
			_ => {}
		}
	}
}

impl Term {
	/// ConEmu's OSC 9 commands: 3 names the tab, 9 sets the working
	/// directory and 12 marks a prompt. The rest sleep, show dialogs, run
	/// programs or reveal the environment, so they are ignored.
	pub(super) fn osc_conemu(&mut self, params: &[&[u8]]) {
		let text = || unquote(&params[1..].join(&b';'));
		match params[0] {
			b"3" => {
				let name = super::osc::clean_title(&text());
				let name = (!name.is_empty()).then_some(name);
				if self.tab_title != name {
					self.tab_title = name;
					self.title_generation = self.title_generation.wrapping_add(1);
				}
			}
			b"9" => self.set_cwd_path(&text()),
			b"12" => self.shell_marker(&[b"A"]),
			_ => {}
		}
	}

	/// An OSC 133 marker from another protocol: A also starts a prompt for
	/// the status bar, but there is no fresh line.
	fn shell_marker(&mut self, params: &[&[u8]]) {
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

	/// Sets the working directory to an absolute path without control
	/// characters; anything else is ignored.
	pub(super) fn set_cwd_path(&mut self, path: &[u8]) {
		if let Ok(path) = std::str::from_utf8(path)
			&& path.starts_with('/')
			&& !path.chars().any(char::is_control)
			&& self.cwd != std::path::Path::new(path)
		{
			self.cwd = path.into();
			self.cwd_generation = self.cwd_generation.wrapping_add(1);
		}
	}
}

/// `"text"` without its quotes; unquoted text is kept as it is.
fn unquote(text: &[u8]) -> Vec<u8> {
	match text {
		[b'"', inner @ .., b'"'] => inner.to_vec(),
		_ => text.to_vec(),
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
