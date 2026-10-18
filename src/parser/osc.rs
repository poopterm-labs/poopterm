//! Operating System Commands: titles, colors, hyperlinks, clipboard, cwd.
use std::sync::Arc;

use crate::{
	shell_integration::decode_uri_path,
	term::{NO_EXTRA, Term},
};

impl Term {
	pub(super) fn apply_osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
		let Some(command) = params.first().copied() else {
			return;
		};
		match command {
			b"52" if params.len() >= 3 && params[1] == b"c" && params[2] != b"?" => {
				// OSC 52 clipboard reads are deliberately not answered: doing so would
				// disclose the user's clipboard to any process running in the shell.
				if let Some(bytes) = decode_base64(params[2])
					&& let Ok(text) = String::from_utf8(bytes)
				{
					self.clipboard_write = Some(text);
				}
			}
			b"8" if params.len() >= 3 => {
				let uri = params[2];
				if uri.is_empty() {
					self.active_hyperlink = 0;
					self.active_extra = NO_EXTRA;
				} else if uri.len() <= 2048
					&& let Ok(uri) = std::str::from_utf8(uri)
					&& !uri.chars().any(char::is_control)
					&& self.hyperlinks.len() < 4096
				{
					self.hyperlinks.push(uri.to_owned());
					self.active_hyperlink = (self.hyperlinks.len() - 1) as u16;
					self.active_extra = Arc::make_mut(&mut self.extras)
						.with_hyperlink(NO_EXTRA, self.active_hyperlink)
						.unwrap_or_default();
				}
			}
			b"133" => self.shell.record(
				params,
				self.history_base + self.scrollback.len() + self.cy,
				self.cx,
				std::time::Instant::now(),
			),
			b"0" | b"1" | b"2" if params.len() >= 2 => {
				let title = String::from_utf8_lossy(params[1]);
				let title: String = title.chars().filter(|c| !c.is_control()).take(64).collect();
				if self.title != title {
					self.title = title;
					self.title_generation = self.title_generation.wrapping_add(1);
				}
			}
			b"4" => self.osc_palette(&params[1..]),
			b"104" => {
				if params.len() == 1 {
					self.color_overrides.fill(None);
				} else {
					for index in params[1..]
						.iter()
						.filter_map(|value| std::str::from_utf8(value).ok()?.parse::<usize>().ok())
					{
						if index < self.color_overrides.len() {
							self.color_overrides[index] = None;
						}
					}
				}
			}
			b"7" if params.len() >= 2 => {
				if let Ok(uri) = std::str::from_utf8(params[1])
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
			b"10" | b"11" if params.len() >= 2 => {
				let foreground = command == b"10";
				if params[1] == b"?" {
					let color = if foreground {
						self.default_fg
					} else {
						self.default_bg
					};
					self.responses.push(osc_color_reply(command, color));
				} else if let Some(color) = parse_osc_color(params[1]) {
					if foreground {
						self.default_fg = color;
						self.fg = color;
					} else {
						self.default_bg = color;
						self.bg = color;
					}
				}
			}
			_ => {}
		}
	}

	fn osc_palette(&mut self, params: &[&[u8]]) {
		let mut i = 0;
		while i + 1 < params.len() {
			let Ok(index) = std::str::from_utf8(params[i])
				.unwrap_or("")
				.parse::<usize>()
			else {
				i += 2;
				continue;
			};
			if index < self.color_overrides.len() {
				if params[i + 1] == b"?" {
					let color = self.indexed_color(index as u8);
					self.responses.push(
						format!(
							"\x1b]4;{index};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
							(color >> 16) & 0xff,
							(color >> 8) & 0xff,
							color & 0xff
						)
						.into_bytes(),
					);
				} else if let Some(color) = parse_osc_color(params[i + 1]) {
					self.color_overrides[index] = Some(color);
				}
			}
			i += 2;
		}
	}
}

fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
	const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
	if input.len() > MAX_CLIPBOARD_BYTES.saturating_mul(4) / 3 + 4 || input.len() % 4 != 0 {
		return None;
	}
	let mut output = Vec::with_capacity(input.len() / 4 * 3);
	for (index, chunk) in input.as_chunks::<4>().0.iter().enumerate() {
		let last = index + 1 == input.len() / 4;
		let value = |byte| match byte {
			b'A'..=b'Z' => Some(byte - b'A'),
			b'a'..=b'z' => Some(byte - b'a' + 26),
			b'0'..=b'9' => Some(byte - b'0' + 52),
			b'+' => Some(62),
			b'/' => Some(63),
			_ => None,
		};
		let a = value(chunk[0])? as u32;
		let b = value(chunk[1])? as u32;
		let c = if chunk[2] == b'=' {
			if !last || chunk[3] != b'=' || b & 0x0f != 0 {
				return None;
			}
			None
		} else {
			Some(value(chunk[2])? as u32)
		};
		let d = if chunk[3] == b'=' {
			if !last || c.is_none() || c.is_some_and(|value| value & 0x03 != 0) {
				return None;
			}
			None
		} else {
			Some(value(chunk[3])? as u32)
		};
		let bits = (a << 18) | (b << 12) | (c.unwrap_or(0) << 6) | d.unwrap_or(0);
		output.push((bits >> 16) as u8);
		if c.is_some() {
			output.push((bits >> 8) as u8);
		}
		if d.is_some() {
			output.push(bits as u8);
		}
		if output.len() > MAX_CLIPBOARD_BYTES {
			return None;
		}
	}
	Some(output)
}

fn osc_color_reply(command: &[u8], color: u32) -> Vec<u8> {
	format!(
		"\x1b]{};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
		String::from_utf8_lossy(command),
		(color >> 16) & 0xff,
		(color >> 8) & 0xff,
		color & 0xff
	)
	.into_bytes()
}

fn parse_osc_color(value: &[u8]) -> Option<u32> {
	if let Ok(value) = std::str::from_utf8(value)
		&& let Some(hex) = value.strip_prefix('#')
		&& hex.len() == 6
	{
		return u32::from_str_radix(hex, 16).ok();
	}
	if let Ok(value) = std::str::from_utf8(value)
		&& let Some(rgb) = value.strip_prefix("rgb:")
	{
		let mut channels = rgb
			.split('/')
			.map(|part| u16::from_str_radix(part, 16).ok());
		let channel = |value: u16, digits: usize| -> u32 {
			let max = (1u32 << (digits.min(4) * 4)) - 1;
			((value as u32 * 255 + max / 2) / max).min(255)
		};
		let r = channels.next()??;
		let g = channels.next()??;
		let b = channels.next()??;
		return Some(
			(channel(r, rgb.split('/').next()?.len()) << 16)
				| (channel(g, rgb.split('/').nth(1)?.len()) << 8)
				| channel(b, rgb.split('/').nth(2)?.len()),
		);
	}
	None
}
