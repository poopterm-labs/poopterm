//! Clipboard OSCs: xterm's OSC 52 and kitty's OSC 5522. Writes are queued
//! for the UI thread; reads are refused, since answering would disclose the
//! user's clipboard to any program running in the terminal.
use std::sync::Arc;

use crate::{
	graphics::base64,
	term::{
		Term,
		clipboard::{ClipboardWrite, Selection, Upload, WriteError},
	},
};

/// Most bytes an OSC 52 write may carry.
const MAX_OSC52_BYTES: usize = 1024 * 1024;

impl Term {
	/// OSC 52: `targets` names selections (`c` clipboard, `p` primary, `s`
	/// the clipboard here; empty means `s0`), `data` is base64 or `?`.
	pub(super) fn osc_selection(&mut self, targets: &[u8], data: &[u8]) {
		if data == b"?" || data.len() > MAX_OSC52_BYTES.div_ceil(3) * 4 {
			return;
		}
		// Data that is not base64 clears the selection.
		let text: Arc<[u8]> = base64::decode(data).unwrap_or_default().into();
		let targets = if targets.is_empty() { b"s0" } else { targets };
		let mut selections = Vec::new();
		for &target in targets {
			let selection = match target {
				b'c' | b's' => Selection::Clipboard,
				b'p' => Selection::Primary,
				_ => continue,
			};
			if !selections.contains(&selection) {
				selections.push(selection);
			}
		}
		for selection in selections {
			self.queue_clipboard_write(ClipboardWrite {
				selection,
				items: vec![("text/plain".to_owned(), text.clone())],
			});
		}
	}

	/// OSC 5522: `metadata` is `key=value` pairs separated by `:`.
	pub(super) fn osc_kitty_clipboard(&mut self, metadata: &[u8], payload: &[u8]) {
		let mut kind = &b""[..];
		let (mut mime, mut location, mut id) = (None, &b""[..], String::new());
		for pair in metadata.split(|&byte| byte == b':') {
			let Some(at) = pair.iter().position(|&byte| byte == b'=') else {
				continue;
			};
			let (key, value) = (&pair[..at], &pair[at + 1..]);
			match key {
				b"type" => kind = value,
				b"mime" => mime = Some(value),
				b"loc" => location = value,
				// Echoed back, so only safe characters are kept.
				b"id" => {
					id = value
						.iter()
						.filter(|&&byte| byte.is_ascii_alphanumeric() || b"-_+.".contains(&byte))
						.take(64)
						.map(|&byte| byte as char)
						.collect();
				}
				_ => {}
			}
		}
		match kind {
			b"read" => self.clipboard_reply("read", "EPERM", &id),
			b"write" => {
				let selection = match location {
					b"primary" if cfg!(target_os = "macos") => {
						self.clipboard_upload = None;
						return self.clipboard_reply("write", "ENOSYS", &id);
					}
					b"primary" => Selection::Primary,
					_ => Selection::Clipboard,
				};
				self.clipboard_upload = Some(Upload::new(selection, id));
			}
			b"wdata" => self.clipboard_data(mime, payload),
			b"walias" => {
				if let Some(upload) = &mut self.clipboard_upload
					&& let Some(target) = mime.and_then(decode_text)
					&& let Some(aliases) = decode_text(payload)
				{
					upload.alias(target, &aliases);
				}
			}
			_ => {}
		}
	}

	/// A `wdata` packet: data for `mime`, or the end of the write.
	fn clipboard_data(&mut self, mime: Option<&[u8]>, payload: &[u8]) {
		let Some(upload) = &mut self.clipboard_upload else {
			return;
		};
		let result = match mime {
			None if payload.is_empty() => {
				let upload = self.clipboard_upload.take().unwrap();
				let id = upload.id.clone();
				match upload.finish() {
					Ok(write) => {
						self.queue_clipboard_write(write);
						return self.clipboard_reply("write", "DONE", &id);
					}
					Err(error) => Err((error, id)),
				}
			}
			Some(mime) => match decode_text(mime) {
				Some(mime) => upload
					.data(mime, payload)
					.map_err(|error| (error, upload.id.clone())),
				None => Err((WriteError::Invalid, upload.id.clone())),
			},
			None => Err((WriteError::Invalid, upload.id.clone())),
		};
		// After an error, packets are ignored until the next write.
		if let Err((error, id)) = result {
			self.clipboard_upload = None;
			self.clipboard_reply("write", error.status(), &id);
		}
	}

	fn clipboard_reply(&mut self, kind: &str, status: &str, id: &str) {
		let id = if id.is_empty() {
			String::new()
		} else {
			format!(":id={id}")
		};
		self
			.responses
			.push(format!("\x1b]5522;type={kind}:status={status}{id}\x1b\\").into_bytes());
	}
}

/// A padded base64 value holding UTF-8 text without control characters.
fn decode_text(value: &[u8]) -> Option<String> {
	if value.len() % 4 != 0 || value.len() > 1024 {
		return None;
	}
	let text = String::from_utf8(base64::decode(value)?).ok()?;
	(!text.chars().any(char::is_control)).then_some(text)
}
