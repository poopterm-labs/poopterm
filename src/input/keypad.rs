//! The numeric keypad in application mode (DECKPAM), xterm's PC-style table.
//!
//! Operators, Enter and `=` send `SS3` sequences; digits and the period
//! follow NumLock, as on xterm: with NumLock on they type themselves, with
//! it off they are the cursor and editing keys they are labeled with.
use super::{
	keys::{KeyModes, Modifiers},
	modify::{CURSOR_KEYS, KEYPAD_KEYS, with_modifiers},
};
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
		let plain = format!("\x1b{}E", if modes.application_cursor { 'O' } else { '[' });
		let level = modes.options.modify[CURSOR_KEYS];
		return Some(with_modifiers(level, None, param, 'E', plain.into()));
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
			// With NumLock ignored (?1035 reset), digits are keypad keys too.
			"." if !modes.keyboard.numlock => 'n',
			digit @ ("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
				if !modes.keyboard.numlock =>
			{
				char::from(b'p' + (digit.as_bytes()[0] - b'0'))
			}
			_ => return None,
		},
		_ => return None,
	};
	let level = modes.options.modify[KEYPAD_KEYS];
	let plain = format!("\x1bO{letter}").into_bytes();
	Some(with_modifiers(level, None, param, letter, plain))
}

#[cfg(test)]
mod tests {
	use super::*;

	const APPLICATION: KeyModes = KeyModes {
		application_cursor: false,
		application_keypad: true,
		kitty: false,
		newline: false,
		options: crate::input::modify::KeyOptions {
			modify: [0, 2, 2, 0, 0, 0, 0, 0],
			format: [0; 8],
			other_mask: 0,
		},
		keyboard: crate::input::keyboard_modes::KeyboardModes {
			auto_repeat: true,
			backarrow_bs: false,
			eight_bit_meta: true,
			numlock: true,
			meta_escape: true,
			delete_is_del: false,
			alt_escape: false,
		},
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
