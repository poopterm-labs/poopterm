//! State behind iTerm2's OSC 1337 commands: user variables, the remote
//! host, the cursor guide, text being copied to the clipboard, and requests
//! for the window.

/// Most user variables kept; new names beyond it are ignored.
pub(crate) const MAX_USER_VARS: usize = 64;
/// Longest user variable name, in bytes.
pub(crate) const MAX_NAME: usize = 64;
/// Largest user variable value, in bytes.
pub(crate) const MAX_VALUE: usize = 4096;
/// Most text a `CopyToClipboard` captures, as for OSC 52.
pub(crate) const MAX_CAPTURE: usize = 1024 * 1024;

/// This machine's host name, looked up once.
pub(crate) fn hostname() -> &'static str {
	static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
	NAME.get_or_init(|| {
		let mut buffer = [0u8; 256];
		// SAFETY: the buffer is writable for its whole length.
		let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
		let end = buffer
			.iter()
			.position(|&byte| byte == 0)
			.unwrap_or(buffer.len());
		if result == 0 {
			String::from_utf8_lossy(&buffer[..end]).into_owned()
		} else {
			String::new()
		}
	})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attention {
	/// Until the user looks (`yes`).
	Critical,
	/// Once (`once`, `fireworks`).
	Informational,
}

/// What a program asked of the window; the UI thread takes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowRequest {
	Focus,
	/// `None` cancels an earlier request.
	Attention(Option<Attention>),
}

#[derive(Clone, Default)]
pub(crate) struct Iterm {
	pub cursor_guide: bool,
	/// `user@host` from `RemoteHost`.
	pub remote_host: Option<String>,
	user_vars: Vec<(String, String)>,
	/// Text since `CopyToClipboard`; `None` inside once it grew too large.
	pub capture: Option<Option<String>>,
}

impl Iterm {
	pub(crate) fn user_var(&self, name: &str) -> Option<&str> {
		self
			.user_vars
			.iter()
			.find(|(known, _)| known == name)
			.map(|(_, value)| value.as_str())
	}

	pub(crate) fn set_user_var(&mut self, name: &str, value: String) {
		if name.is_empty() || name.len() > MAX_NAME || value.len() > MAX_VALUE {
			return;
		}
		match self.user_vars.iter().position(|(known, _)| known == name) {
			Some(index) => self.user_vars[index].1 = value,
			None if self.user_vars.len() < MAX_USER_VARS => {
				self.user_vars.push((name.to_owned(), value));
			}
			None => {}
		}
	}

	/// The remote `user@host` when its host is not `local`; names compare
	/// without case and by their first label when one is unqualified.
	pub(crate) fn remote_label(&self, local: &str) -> Option<&str> {
		let remote = self.remote_host.as_deref()?;
		let host = remote.rsplit_once('@').map_or(remote, |(_, host)| host);
		let short = |name: &str| {
			name
				.split('.')
				.next()
				.unwrap_or_default()
				.to_ascii_lowercase()
		};
		let same = host.eq_ignore_ascii_case(local) || short(host) == short(local);
		(!same).then_some(remote)
	}

	/// Adds shown text to a capture in progress.
	#[cold]
	pub(crate) fn capture(&mut self, text: &str) {
		if let Some(Some(captured)) = &mut self.capture {
			if captured.len() + text.len() > MAX_CAPTURE {
				self.capture = Some(None);
			} else {
				captured.push_str(text);
			}
		}
	}
}
