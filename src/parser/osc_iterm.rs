//! iTerm2's OSC 1337 commands besides images. Commands that need a feature
//! poopterm lacks (profiles, the Touch Bar, annotations, blocks, buttons,
//! custom sequences) or a user's confirmation (background images, opening
//! URLs) are ignored.
use std::sync::Arc;

use crate::{
	graphics::base64,
	term::{
		CursorShape, Dynamic, Setting, Term,
		clipboard::{ClipboardWrite, Selection},
		iterm::{Attention, MAX_CAPTURE, WindowRequest},
	},
};

impl Term {
	/// `params` are `Command=value` (or `Command`) and any further fields.
	pub(super) fn osc_iterm(&mut self, params: &[&[u8]], bell: bool) {
		let Ok(first) = std::str::from_utf8(params[0]) else {
			return;
		};
		let (command, value) = first.split_once('=').unwrap_or((first, ""));
		match command {
			"CursorShape" => {
				self.cursor_shape = match value {
					"0" => CursorShape::Block,
					"1" => CursorShape::Bar,
					"2" => CursorShape::Underline,
					_ => return,
				}
			}
			"HighlightCursorLine" => match value {
				"yes" => self.iterm.cursor_guide = true,
				"no" => self.iterm.cursor_guide = false,
				_ => {}
			},
			"SetMark" => {
				let row = self.history_base + self.scrollback.len() + self.cy;
				self.shell.set_mark(row, self.cx);
			}
			"StealFocus" => self.window_requests.push(WindowRequest::Focus),
			"RequestAttention" => {
				let attention = match value {
					"yes" => Some(Attention::Critical),
					"once" | "fireworks" => Some(Attention::Informational),
					"no" => None,
					_ => return,
				};
				self
					.window_requests
					.push(WindowRequest::Attention(attention));
			}
			"ClearScrollback" => self.clear_scrollback(),
			"CurrentDir" => self.set_cwd_path(value.as_bytes()),
			"RemoteHost" => {
				if value.len() <= 255
					&& value
						.split_once('@')
						.is_some_and(|(_, host)| !host.is_empty())
					&& !value.chars().any(char::is_control)
				{
					self.iterm.remote_host = Some(value.to_owned());
				}
			}
			"SetUserVar" => {
				if let Some((name, encoded)) = value.split_once('=')
					&& !name.chars().any(char::is_control)
					&& encoded.len() <= crate::term::iterm::MAX_VALUE.div_ceil(3) * 4
					&& let Some(decoded) = base64::decode(encoded.as_bytes())
					&& let Ok(decoded) = String::from_utf8(decoded)
				{
					self.iterm.set_user_var(name, decoded);
				}
			}
			"ReportVariable" => {
				let name = base64::decode(value.as_bytes()).unwrap_or_default();
				// Only what programs set themselves is reported.
				let answer = std::str::from_utf8(&name)
					.ok()
					.and_then(|name| name.strip_prefix("user."))
					.and_then(|name| self.iterm.user_var(name))
					.map(|value| base64::encode(value.as_bytes(), true))
					.unwrap_or_default();
				self.osc_reply(format!("1337;ReportVariable={answer}"), bell);
			}
			"Copy" => {
				if let Some(encoded) = value.strip_prefix(':')
					&& encoded.len() <= MAX_CAPTURE.div_ceil(3) * 4
					&& let Some(text) = base64::decode(encoded.as_bytes())
				{
					self.copy_text(text.into());
				}
			}
			// Only the general pasteboard exists here.
			"CopyToClipboard" if value.is_empty() => self.iterm.capture = Some(Some(String::new())),
			"EndCopy" => {
				if let Some(Some(text)) = self.iterm.capture.take() {
					self.copy_text(text.into_bytes().into());
				}
			}
			"SetColors" => {
				if let Some((key, color)) = value.split_once('=')
					&& let Some(color) = iterm_color(color)
				{
					self.set_iterm_color(key, color);
				}
			}
			"ReportCellSize" => {
				let scale = self.scale.max(0.1);
				let (height, width) = (
					self.cell_height as f32 / scale,
					self.cell_width as f32 / scale,
				);
				self.osc_reply(
					format!("1337;ReportCellSize={height:.2};{width:.2};{scale:.1}"),
					bell,
				);
			}
			// Uploads need a file picker, which poopterm does not have.
			"RequestUpload" => self.responses.push(b"abort\n".to_vec()),
			_ => {}
		}
	}

	fn copy_text(&mut self, text: Arc<[u8]>) {
		self.queue_clipboard_write(ClipboardWrite {
			selection: Selection::Clipboard,
			items: vec![("text/plain".to_owned(), text)],
		});
	}

	/// `SetColors` for the keys poopterm has colors for.
	fn set_iterm_color(&mut self, key: &str, color: u32) {
		const NAMES: [&str; 8] = [
			"black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
		];
		let indexed = |name: &str| NAMES.iter().position(|known| *known == name);
		match key {
			"fg" => self.set_dynamic_color(Dynamic::Foreground, Some(color)),
			"bg" => self.set_dynamic_color(Dynamic::Background, Some(color)),
			"selbg" => self.set_dynamic_color(Dynamic::HighlightBackground, Some(color)),
			"selfg" => self.set_dynamic_color(Dynamic::HighlightForeground, Some(color)),
			"curbg" => self.set_dynamic_color(Dynamic::Cursor, Some(color)),
			"curfg" => self.colors.cursor_text = Setting::Color(color),
			// The special colors of OSC 5, switched on as OSC 6 would.
			"bold" | "underline" => {
				let slot = usize::from(key == "underline");
				self.colors.special[slot] = Some(color);
				self.colors.special_modes[slot] = true;
			}
			_ => {
				let index = match key.strip_prefix("br_") {
					Some(name) => indexed(name).map(|index| index + 8),
					None => indexed(key),
				};
				if let Some(index) = index {
					self.color_overrides[index] = Some(color);
				}
			}
		}
	}
}

/// `RGB` or `RRGGBB` hex, optionally after a `srgb:`, `rgb:` or `p3:` color
/// space, which is treated as sRGB.
fn iterm_color(value: &str) -> Option<u32> {
	let hex = match value.split_once(':') {
		Some(("srgb" | "rgb" | "p3", hex)) => hex,
		Some(_) => return None,
		None => value,
	};
	if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
		return None;
	}
	let color = u32::from_str_radix(hex, 16).ok()?;
	match hex.len() {
		6 => Some(color),
		// Each digit is repeated: `f00` is `ff0000`.
		3 => {
			let [r, g, b] = [color >> 8, (color >> 4) & 0xf, color & 0xf];
			Some(((r * 0x11) << 16) | ((g * 0x11) << 8) | (b * 0x11))
		}
		_ => None,
	}
}
