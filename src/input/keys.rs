//! Translating winit key events into the bytes a terminal application expects.
use super::keyboard_modes::{KeyboardModes, Meta};
use super::modify::{CURSOR_KEYS, FUNCTION_KEYS, KeyOptions, encode_other_key, with_modifiers};
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
	/// DECKPAM (`ESC =`): the numeric keypad sends `SS3` sequences.
	pub application_keypad: bool,
	/// Kitty keyboard protocol disambiguation (`CSI > 1 u`).
	pub kitty: bool,
	/// LNM (`CSI 20 h`): Enter sends CR LF.
	pub newline: bool,
	/// XTMODKEYS and XTFMTKEYS choices.
	pub options: KeyOptions,
	/// DECBKM and xterm's Meta, NumLock and Delete modes.
	pub keyboard: KeyboardModes,
}

/// Encodes one key press, or returns `None` when the key produces no input.
pub(crate) fn encode_key(key: &Key, mods: Modifiers, modes: KeyModes) -> Option<Vec<u8>> {
	// xterm's modifier parameter: 1 + shift + 2·alt + 4·ctrl + 8·super.
	let param = 1
		+ u8::from(mods.shift)
		+ 2 * u8::from(mods.alt)
		+ 4 * u8::from(mods.ctrl)
		+ 8 * u8::from(mods.super_key);
	// The kitty protocol always reports modifiers as the second parameter.
	let modify = modes.options.modify;
	let levels = if modes.kitty {
		(2, 2)
	} else {
		(modify[CURSOR_KEYS], modify[FUNCTION_KEYS])
	};
	let del = modes.keyboard.delete_is_del && !modes.kitty;
	if del && param == 1 && *key == Key::Named(NamedKey::Delete) {
		return Some(vec![0x7f]);
	}
	if let Key::Named(named) = key
		&& let Some(sequence) = special_key(*named, param, modes.application_cursor, levels)
	{
		return Some(sequence);
	}
	if modes.kitty
		&& let Some(sequence) = kitty_key(key, mods, param)
	{
		return Some(sequence);
	}
	if !modes.kitty
		&& let Some(sequence) = encode_other_key(key, mods, modes.options)
	{
		return Some(sequence);
	}
	let mut bytes = legacy_key(key, mods)?;
	match key {
		Key::Named(NamedKey::Enter) if modes.newline => bytes.push(b'\n'),
		Key::Named(NamedKey::Backspace) if modes.keyboard.backarrow_bs => bytes = vec![0x08],
		_ => {}
	}
	if mods.alt && !modes.kitty {
		match (modes.keyboard.meta(), bytes.as_slice()) {
			(Meta::Escape, _) => bytes.insert(0, 0x1b),
			// A lone byte is ASCII; characters beyond it have no free bit.
			(Meta::EightBit, &[byte]) => {
				bytes = char::from(byte | 0x80).to_string().into_bytes();
			}
			_ => {}
		}
	}
	Some(bytes)
}

/// Cursor, editing and function keys, which use the same `CSI` forms in
/// legacy xterm mode and the kitty protocol. `cursor` and `function` are the
/// modifyCursorKeys and modifyFunctionKeys levels.
fn special_key(
	key: NamedKey,
	param: u8,
	application_cursor: bool,
	(cursor, function): (i8, i8),
) -> Option<Vec<u8>> {
	let letter = match key {
		NamedKey::ArrowUp => Some('A'),
		NamedKey::ArrowDown => Some('B'),
		NamedKey::ArrowRight => Some('C'),
		NamedKey::ArrowLeft => Some('D'),
		NamedKey::Home => Some('H'),
		NamedKey::End => Some('F'),
		_ => None,
	};
	if let Some(letter) = letter {
		let plain = format!("\x1b{}{letter}", if application_cursor { 'O' } else { '[' });
		return Some(with_modifiers(cursor, None, param, letter, plain.into()));
	}
	if let Some(letter) = match key {
		NamedKey::F1 => Some('P'),
		NamedKey::F2 => Some('Q'),
		NamedKey::F3 => Some('R'),
		NamedKey::F4 => Some('S'),
		_ => None,
	} {
		let plain = format!("\x1bO{letter}").into_bytes();
		return Some(with_modifiers(function, None, param, letter, plain));
	}
	let number = match key {
		NamedKey::Insert => 2,
		NamedKey::Delete => 3,
		NamedKey::PageUp => 5,
		NamedKey::PageDown => 6,
		NamedKey::F5 => 15,
		NamedKey::F6 => 17,
		NamedKey::F7 => 18,
		NamedKey::F8 => 19,
		NamedKey::F9 => 20,
		NamedKey::F10 => 21,
		NamedKey::F11 => 23,
		NamedKey::F12 => 24,
		_ => return None,
	};
	let plain = format!("\x1b[{number}~").into_bytes();
	Some(with_modifiers(function, Some(number), param, '~', plain))
}

/// Kitty "disambiguate escape codes": modified text keys and Escape become `CSI u`.
fn kitty_key(key: &Key, mods: Modifiers, param: u8) -> Option<Vec<u8>> {
	match key {
		Key::Named(NamedKey::Escape) if param > 1 => Some(format!("\x1b[27;{param}u").into_bytes()),
		Key::Character(text) if mods.ctrl || mods.alt || mods.super_key => {
			let ch = text.chars().next()?.to_ascii_lowercase();
			Some(format!("\x1b[{};{param}u", ch as u32).into_bytes())
		}
		_ => None,
	}
}

/// Keys that produce plain characters or C0 controls.
fn legacy_key(key: &Key, mods: Modifiers) -> Option<Vec<u8>> {
	match key {
		Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
		Key::Named(NamedKey::Backspace) => Some(vec![0x7f]),
		Key::Named(NamedKey::Tab) if mods.shift => Some(b"\x1b[Z".to_vec()),
		Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
		Key::Named(NamedKey::Escape) => Some(vec![0x1b]),
		Key::Named(NamedKey::Space) => Some(vec![if mods.ctrl { 0 } else { b' ' }]),
		Key::Character(text) if mods.ctrl && text.len() == 1 => {
			control_byte(text.as_bytes()[0]).map(|b| vec![b])
		}
		Key::Character(text) => Some(text.as_bytes().to_vec()),
		_ => None,
	}
}

/// The C0 control a Ctrl+key chord produces, following xterm's mapping.
fn control_byte(key: u8) -> Option<u8> {
	if key.is_ascii_alphabetic() {
		return Some(key.to_ascii_lowercase() & 0x1f);
	}
	match key {
		b' ' | b'@' | b'2' => Some(0),
		b'[' | b'3' => Some(27),
		b'\\' | b'4' => Some(28),
		b']' | b'5' => Some(29),
		b'^' | b'6' => Some(30),
		b'_' | b'/' | b'7' => Some(31),
		b'?' | b'8' => Some(127),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn named(key: NamedKey) -> Key {
		Key::Named(key)
	}

	fn legacy(key: Key, mods: Modifiers) -> Vec<u8> {
		encode_key(&key, mods, KeyModes::default()).unwrap()
	}

	const CTRL: Modifiers = Modifiers {
		ctrl: true,
		shift: false,
		alt: false,
		super_key: false,
	};
	const ALT: Modifiers = Modifiers {
		ctrl: false,
		shift: false,
		alt: true,
		super_key: false,
	};

	#[test]
	fn alt_prefixes_characters_with_escape() {
		assert_eq!(legacy(Key::Character("b".into()), ALT), b"\x1bb");
		assert_eq!(legacy(named(NamedKey::Backspace), ALT), b"\x1b\x7f");
		let ctrl_alt = Modifiers { alt: true, ..CTRL };
		assert_eq!(legacy(Key::Character("c".into()), ctrl_alt), b"\x1b\x03");
	}

	#[test]
	fn modified_cursor_keys_use_xterm_parameters() {
		assert_eq!(legacy(named(NamedKey::ArrowLeft), CTRL), b"\x1b[1;5D");
		assert_eq!(legacy(named(NamedKey::End), ALT), b"\x1b[1;3F");
		assert_eq!(legacy(named(NamedKey::Delete), CTRL), b"\x1b[3;5~");
		assert_eq!(legacy(named(NamedKey::F5), CTRL), b"\x1b[15;5~");
	}

	#[test]
	fn application_cursor_mode_applies_to_home_and_end() {
		let modes = KeyModes {
			application_cursor: true,
			..KeyModes::default()
		};
		let none = Modifiers::default();
		assert_eq!(
			encode_key(&named(NamedKey::Home), none, modes).unwrap(),
			b"\x1bOH"
		);
		assert_eq!(
			encode_key(&named(NamedKey::ArrowUp), none, modes).unwrap(),
			b"\x1bOA"
		);
	}

	#[test]
	fn newline_mode_makes_enter_send_cr_lf() {
		let newline = KeyModes {
			newline: true,
			..KeyModes::default()
		};
		let none = Modifiers::default();
		let enter = named(NamedKey::Enter);
		assert_eq!(encode_key(&enter, none, newline).unwrap(), b"\r\n");
		assert_eq!(
			encode_key(&enter, ALT, newline).unwrap(),
			b"\x1b\r\n",
			"Alt still prefixes ESC"
		);
		assert_eq!(
			encode_key(&Key::Character("m".into()), CTRL, newline).unwrap(),
			b"\r",
			"Ctrl+M is a carriage return, not Enter"
		);
		assert_eq!(legacy(enter, none), b"\r", "without LNM");
	}

	#[test]
	fn plain_keys_are_unchanged() {
		let none = Modifiers::default();
		assert_eq!(legacy(Key::Character("x".into()), none), b"x");
		assert_eq!(legacy(Key::Character("c".into()), CTRL), b"\x03");
		assert_eq!(legacy(named(NamedKey::ArrowLeft), none), b"\x1b[D");
		assert_eq!(legacy(named(NamedKey::Enter), none), b"\r");
	}

	#[test]
	fn kitty_mode_disambiguates_modified_text_keys() {
		let kitty = KeyModes {
			kitty: true,
			..KeyModes::default()
		};
		let shift = Modifiers {
			shift: true,
			..Default::default()
		};
		let encode = |key: Key, mods| encode_key(&key, mods, kitty).unwrap();
		assert_eq!(encode(Key::Character("c".into()), CTRL), b"\x1b[99;5u");
		assert_eq!(encode(Key::Character("x".into()), ALT), b"\x1b[120;3u");
		assert_eq!(encode(named(NamedKey::Escape), shift), b"\x1b[27;2u");
		assert_eq!(
			encode(named(NamedKey::Escape), Modifiers::default()),
			b"\x1b"
		);
		assert_eq!(encode(named(NamedKey::ArrowUp), CTRL), b"\x1b[1;5A");
	}
}
