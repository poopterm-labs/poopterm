//! The numeric keypad in application mode (DECKPAM), xterm's PC-style table.
//!
//! Operators, Enter and `=` send `SS3` sequences; digits and the period
//! follow NumLock, as on xterm: with NumLock on they type themselves, with
//! it off they are the cursor and editing keys they are labeled with.
use super::keys::{KeyModes, Modifiers};
use winit::keyboard::{Key, NamedKey};

/// Encodes a key pressed on the numeric keypad, or returns `None` when it
/// sends what the same key elsewhere on the keyboard sends.
pub(crate) fn encode_keypad(key: &Key, mods: Modifiers, modes: KeyModes) -> Option<Vec<u8>> {
	let param = 1
		+ u8::from(mods.shift)
		+ 2 * u8::from(mods.alt)
		+ 4 * u8::from(mods.ctrl)
		+ 8 * u8::from(mods.super_key);
	// Begin (keypad 5 without NumLock) is a cursor key like Home and End.
	if let Key::Named(NamedKey::Clear) = key {
		return Some(ss3_or_csi('E', param, modes.application_cursor));
	}
	if !modes.application_keypad {
		return None;
	}
	let letter = match key {
		Key::Named(NamedKey::Enter) => 'M',
		Key::Named(NamedKey::Tab) => 'I',
		Key::Named(NamedKey::Space) => ' ',
		Key::Character(text) => match text.as_str() {
			"*" => 'j',
			"+" => 'k',
			"," => 'l',
			"-" => 'm',
			"/" => 'o',
			"=" => 'X',
			_ => return None,
		},
		_ => return None,
	};
	Some(if param > 1 {
		format!("\x1bO{param}{letter}").into_bytes()
	} else {
		format!("\x1bO{letter}").into_bytes()
	})
}

fn ss3_or_csi(letter: char, param: u8, application_cursor: bool) -> Vec<u8> {
	match (param > 1, application_cursor) {
		(true, _) => format!("\x1b[1;{param}{letter}"),
		(false, true) => format!("\x1bO{letter}"),
		(false, false) => format!("\x1b[{letter}"),
	}
	.into_bytes()
}

#[cfg(test)]
mod tests {
	use super::*;

	const APPLICATION: KeyModes = KeyModes {
		application_cursor: false,
		application_keypad: true,
		kitty: false,
		newline: false,
	};

	fn text(s: &str) -> Key {
		Key::Character(s.into())
	}

	fn plain(key: Key) -> Option<Vec<u8>> {
		encode_keypad(&key, Modifiers::default(), APPLICATION)
	}

	#[test]
	fn operators_send_ss3_in_application_mode() {
		let expected: [(&str, &[u8]); 6] = [
			("*", b"\x1bOj"),
			("+", b"\x1bOk"),
			(",", b"\x1bOl"),
			("-", b"\x1bOm"),
			("/", b"\x1bOo"),
			("=", b"\x1bOX"),
		];
		for (key, bytes) in expected {
			assert_eq!(plain(text(key)).as_deref(), Some(bytes), "keypad {key}");
		}
	}

	#[test]
	fn enter_tab_and_space_send_ss3_in_application_mode() {
		assert_eq!(plain(Key::Named(NamedKey::Enter)).unwrap(), b"\x1bOM");
		assert_eq!(plain(Key::Named(NamedKey::Tab)).unwrap(), b"\x1bOI");
		assert_eq!(plain(Key::Named(NamedKey::Space)).unwrap(), b"\x1bO ");
	}

	#[test]
	fn numlock_digits_and_period_type_themselves() {
		for key in ["0", "5", "9", "."] {
			assert_eq!(plain(text(key)), None, "keypad {key}");
		}
	}

	#[test]
	fn numeric_mode_leaves_every_key_to_the_main_encoder() {
		let numeric = KeyModes::default();
		for key in [text("+"), text("7"), Key::Named(NamedKey::Enter)] {
			assert_eq!(encode_keypad(&key, Modifiers::default(), numeric), None);
		}
	}

	#[test]
	fn navigation_keys_without_numlock_are_left_to_the_main_encoder() {
		for named in [
			NamedKey::ArrowUp,
			NamedKey::Home,
			NamedKey::Insert,
			NamedKey::Delete,
		] {
			assert_eq!(plain(Key::Named(named)), None, "{named:?}");
		}
	}

	#[test]
	fn modifiers_go_between_ss3_and_the_final() {
		let ctrl = Modifiers {
			ctrl: true,
			..Default::default()
		};
		let shift_alt = Modifiers {
			shift: true,
			alt: true,
			..Default::default()
		};
		assert_eq!(
			encode_keypad(&Key::Named(NamedKey::Enter), ctrl, APPLICATION).unwrap(),
			b"\x1bO5M"
		);
		assert_eq!(
			encode_keypad(&text("+"), shift_alt, APPLICATION).unwrap(),
			b"\x1bO4k"
		);
	}

	#[test]
	fn begin_follows_cursor_key_mode_in_both_keypad_modes() {
		let begin = Key::Named(NamedKey::Clear);
		let none = Modifiers::default();
		assert_eq!(
			encode_keypad(&begin, none, KeyModes::default()).unwrap(),
			b"\x1b[E"
		);
		let cursor = KeyModes {
			application_cursor: true,
			..KeyModes::default()
		};
		assert_eq!(encode_keypad(&begin, none, cursor).unwrap(), b"\x1bOE");
		let shift = Modifiers {
			shift: true,
			..none
		};
		assert_eq!(encode_keypad(&begin, shift, cursor).unwrap(), b"\x1b[1;2E");
	}

	#[test]
	fn other_characters_are_not_keypad_keys() {
		assert_eq!(plain(text("a")), None);
		assert_eq!(plain(text("++")), None);
	}
}
