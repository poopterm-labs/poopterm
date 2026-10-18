//! Translating winit key events into the bytes a terminal application expects.
use winit::keyboard::{Key, NamedKey};

/// Modifier keys held while a key was pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Modifiers {
	pub ctrl: bool,
	pub shift: bool,
	pub alt: bool,
	pub super_key: bool,
}

/// Terminal modes that change key encoding.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyModes {
	/// DECCKM (`CSI ? 1 h`): arrows send `ESC O x` instead of `ESC [ x`.
	pub application_cursor: bool,
	/// Kitty keyboard protocol disambiguation (`CSI > 1 u`).
	pub kitty: bool,
}

/// Encodes one key press, or returns `None` when the key produces no input.
pub(crate) fn encode_key(key: &Key, mods: Modifiers, modes: KeyModes) -> Option<Vec<u8>> {
	if modes.kitty {
		let modifiers = 1
			+ u8::from(mods.shift)
			+ 2 * u8::from(mods.alt)
			+ 4 * u8::from(mods.ctrl)
			+ 8 * u8::from(mods.super_key);
		let letter = match key {
			Key::Named(NamedKey::ArrowUp) => Some('A'),
			Key::Named(NamedKey::ArrowDown) => Some('B'),
			Key::Named(NamedKey::ArrowRight) => Some('C'),
			Key::Named(NamedKey::ArrowLeft) => Some('D'),
			Key::Named(NamedKey::Home) => Some('H'),
			Key::Named(NamedKey::End) => Some('F'),
			_ => None,
		};
		if let Some(letter) = letter {
			if modifiers > 1 {
				return Some(format!("\x1b[1;{modifiers}{letter}").into_bytes());
			}
			return Some(match key {
				Key::Named(NamedKey::ArrowUp) if modes.application_cursor => b"\x1bOA".to_vec(),
				Key::Named(NamedKey::ArrowDown) if modes.application_cursor => b"\x1bOB".to_vec(),
				Key::Named(NamedKey::ArrowRight) if modes.application_cursor => b"\x1bOC".to_vec(),
				Key::Named(NamedKey::ArrowLeft) if modes.application_cursor => b"\x1bOD".to_vec(),
				_ => format!("\x1b[{letter}").into_bytes(),
			});
		}
		match key {
			Key::Named(NamedKey::Enter) => return Some(b"\r".to_vec()),
			Key::Named(NamedKey::Backspace) => return Some(vec![0x7f]),
			Key::Named(NamedKey::Tab) if mods.shift => return Some(b"\x1b[Z".to_vec()),
			Key::Named(NamedKey::Tab) => return Some(b"\t".to_vec()),
			Key::Named(NamedKey::Escape) if modifiers > 1 => {
				return Some(format!("\x1b[27;{modifiers}u").into_bytes());
			}
			Key::Named(NamedKey::Escape) => return Some(vec![0x1b]),
			Key::Named(NamedKey::Insert) => {
				return Some(
					format!(
						"\x1b[2{}~",
						if modifiers > 1 {
							format!(";{modifiers}")
						} else {
							String::new()
						}
					)
					.into_bytes(),
				);
			}
			Key::Named(NamedKey::Delete) => {
				return Some(
					format!(
						"\x1b[3{}~",
						if modifiers > 1 {
							format!(";{modifiers}")
						} else {
							String::new()
						}
					)
					.into_bytes(),
				);
			}
			Key::Named(NamedKey::PageUp) => {
				return Some(
					format!(
						"\x1b[5{}~",
						if modifiers > 1 {
							format!(";{modifiers}")
						} else {
							String::new()
						}
					)
					.into_bytes(),
				);
			}
			Key::Named(NamedKey::PageDown) => {
				return Some(
					format!(
						"\x1b[6{}~",
						if modifiers > 1 {
							format!(";{modifiers}")
						} else {
							String::new()
						}
					)
					.into_bytes(),
				);
			}
			Key::Named(n @ (NamedKey::F1 | NamedKey::F2 | NamedKey::F3 | NamedKey::F4)) => {
				let suffix = match n {
					NamedKey::F1 => 'P',
					NamedKey::F2 => 'Q',
					NamedKey::F3 => 'R',
					_ => 'S',
				};
				return Some(if modifiers > 1 {
					format!("\x1b[1;{modifiers}{suffix}").into_bytes()
				} else {
					format!("\x1bO{suffix}").into_bytes()
				});
			}
			Key::Named(
				n @ (NamedKey::F5
				| NamedKey::F6
				| NamedKey::F7
				| NamedKey::F8
				| NamedKey::F9
				| NamedKey::F10
				| NamedKey::F11
				| NamedKey::F12),
			) => {
				let number = match n {
					NamedKey::F5 => 15,
					NamedKey::F6 => 17,
					NamedKey::F7 => 18,
					NamedKey::F8 => 19,
					NamedKey::F9 => 20,
					NamedKey::F10 => 21,
					NamedKey::F11 => 23,
					_ => 24,
				};
				return Some(
					format!(
						"\x1b[{number}{}~",
						if modifiers > 1 {
							format!(";{modifiers}")
						} else {
							String::new()
						}
					)
					.into_bytes(),
				);
			}
			Key::Character(text) if mods.ctrl || mods.alt || mods.super_key => {
				if let Some(ch) = text.chars().next() {
					return Some(
						format!("\x1b[{};{modifiers}u", ch.to_ascii_lowercase() as u32).into_bytes(),
					);
				}
			}
			_ => {}
		}
	}
	match key {
		Key::Named(n) => match n {
			NamedKey::Enter => Some(b"\r".to_vec()),
			NamedKey::Backspace => Some(vec![0x7f]),
			NamedKey::Tab => Some(if mods.shift {
				b"\x1b[Z".to_vec()
			} else {
				b"\t".to_vec()
			}),
			NamedKey::Escape => Some(vec![0x1b]),
			NamedKey::Space => Some(vec![if mods.ctrl { 0 } else { b' ' }]),
			NamedKey::ArrowUp => Some(if modes.application_cursor {
				b"\x1bOA".to_vec()
			} else {
				b"\x1b[A".to_vec()
			}),
			NamedKey::ArrowDown => Some(if modes.application_cursor {
				b"\x1bOB".to_vec()
			} else {
				b"\x1b[B".to_vec()
			}),
			NamedKey::ArrowRight => Some(if modes.application_cursor {
				b"\x1bOC".to_vec()
			} else {
				b"\x1b[C".to_vec()
			}),
			NamedKey::ArrowLeft => Some(if modes.application_cursor {
				b"\x1bOD".to_vec()
			} else {
				b"\x1b[D".to_vec()
			}),
			NamedKey::Home => Some(b"\x1b[H".to_vec()),
			NamedKey::End => Some(b"\x1b[F".to_vec()),
			NamedKey::PageUp => Some(b"\x1b[5~".to_vec()),
			NamedKey::PageDown => Some(b"\x1b[6~".to_vec()),
			NamedKey::Insert => Some(b"\x1b[2~".to_vec()),
			NamedKey::Delete => Some(b"\x1b[3~".to_vec()),
			NamedKey::F1 => Some(b"\x1bOP".to_vec()),
			NamedKey::F2 => Some(b"\x1bOQ".to_vec()),
			NamedKey::F3 => Some(b"\x1bOR".to_vec()),
			NamedKey::F4 => Some(b"\x1bOS".to_vec()),
			NamedKey::F5 => Some(b"\x1b[15~".to_vec()),
			NamedKey::F6 => Some(b"\x1b[17~".to_vec()),
			NamedKey::F7 => Some(b"\x1b[18~".to_vec()),
			NamedKey::F8 => Some(b"\x1b[19~".to_vec()),
			NamedKey::F9 => Some(b"\x1b[20~".to_vec()),
			NamedKey::F10 => Some(b"\x1b[21~".to_vec()),
			NamedKey::F11 => Some(b"\x1b[23~".to_vec()),
			NamedKey::F12 => Some(b"\x1b[24~".to_vec()),
			_ => None,
		},
		Key::Character(s) => {
			let s = s.as_str();
			if mods.ctrl && s.len() == 1 {
				let c = s.as_bytes()[0];
				if c.is_ascii_alphabetic() {
					return Some(vec![c.to_ascii_lowercase() & 0x1f]);
				}
				return match c {
					b' ' | b'@' | b'2' => Some(vec![0]),
					b'[' | b'3' => Some(vec![27]),
					b'\\' | b'4' => Some(vec![28]),
					b']' | b'5' => Some(vec![29]),
					b'^' | b'6' => Some(vec![30]),
					b'_' | b'/' | b'7' => Some(vec![31]),
					b'?' | b'8' => Some(vec![127]),
					_ => None,
				};
			}
			Some(s.as_bytes().to_vec())
		}
		_ => None,
	}
}
