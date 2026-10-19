//! The DEC private keyboard modes as the key encoders apply them.
use super::{
	keyboard_modes::{KeyboardModes, Meta},
	keypad::encode_keypad,
	keys::{KeyModes, Modifiers, encode_key},
};
use winit::keyboard::{Key, NamedKey};

const NONE: Modifiers = Modifiers {
	ctrl: false,
	shift: false,
	alt: false,
	super_key: false,
};
const ALT: Modifiers = Modifiers { alt: true, ..NONE };
const CTRL: Modifiers = Modifiers { ctrl: true, ..NONE };
const SHIFT: Modifiers = Modifiers {
	shift: true,
	..NONE
};

fn with(change: impl FnOnce(&mut KeyboardModes)) -> KeyModes {
	let mut modes = KeyModes::default();
	change(&mut modes.keyboard);
	modes
}

fn send(key: Key, mods: Modifiers, modes: KeyModes) -> Vec<u8> {
	encode_key(&key, mods, modes).unwrap()
}

fn text(s: &str) -> Key {
	Key::Character(s.into())
}

#[test]
fn initial_modes_match_xterm() {
	let modes = KeyboardModes::default();
	assert!(modes.auto_repeat, "DECARM");
	assert!(!modes.backarrow_bs, "DECBKM: Backspace sends DEL");
	assert!(modes.eight_bit_meta);
	assert!(modes.numlock);
	assert!(modes.meta_escape, "Alt has always sent ESC here");
	assert!(!modes.delete_is_del);
	assert!(!modes.alt_escape);
	assert_eq!(modes.meta(), Meta::Escape);
}

#[test]
fn each_mode_number_names_its_flag() {
	let mut modes = KeyboardModes::default();
	for (number, value) in [(8, true), (67, false), (1034, true), (1035, true)] {
		assert_eq!(modes.mode(number).copied(), Some(value), "mode {number}");
	}
	for (number, value) in [(1036, true), (1037, false), (1039, false)] {
		assert_eq!(modes.mode(number).copied(), Some(value), "mode {number}");
	}
	*modes.mode(67).unwrap() = true;
	*modes.mode(8).unwrap() = false;
	assert!(modes.backarrow_bs && !modes.auto_repeat);
	for number in [0, 1, 9, 66, 1033, 1038, 1040, 2004] {
		assert!(modes.mode(number).is_none(), "mode {number}");
	}
}

#[test]
fn meta_follows_the_escape_and_eight_bit_modes() {
	let rows = [
		(true, false, true, Meta::Escape),
		(false, true, false, Meta::Escape),
		(true, true, false, Meta::Escape),
		(false, false, true, Meta::EightBit),
		(false, false, false, Meta::Ignore),
	];
	for (meta_escape, alt_escape, eight_bit_meta, expected) in rows {
		let modes = KeyboardModes {
			meta_escape,
			alt_escape,
			eight_bit_meta,
			..Default::default()
		};
		assert_eq!(
			modes.meta(),
			expected,
			"{meta_escape} {alt_escape} {eight_bit_meta}"
		);
	}
}

#[test]
fn backarrow_mode_makes_backspace_send_bs() {
	let bs = with(|k| k.backarrow_bs = true);
	assert_eq!(send(Key::Named(NamedKey::Backspace), NONE, bs), b"\x08");
	assert_eq!(send(Key::Named(NamedKey::Backspace), ALT, bs), b"\x1b\x08");
	let del = KeyModes::default();
	assert_eq!(send(Key::Named(NamedKey::Backspace), NONE, del), b"\x7f");
}

#[test]
fn delete_is_del_changes_only_the_plain_delete() {
	let del = with(|k| k.delete_is_del = true);
	assert_eq!(send(Key::Named(NamedKey::Delete), NONE, del), b"\x7f");
	assert_eq!(send(Key::Named(NamedKey::Delete), CTRL, del), b"\x1b[3;5~");
	assert_eq!(send(Key::Named(NamedKey::Insert), NONE, del), b"\x1b[2~");
	assert_eq!(
		send(Key::Named(NamedKey::Delete), NONE, KeyModes::default()),
		b"\x1b[3~"
	);
}

#[test]
fn eight_bit_meta_sets_the_high_bit_of_ascii() {
	let eight = with(|k| k.meta_escape = false);
	assert_eq!(send(text("a"), ALT, eight), "\u{e1}".as_bytes());
	assert_eq!(
		send(text("A"), Modifiers { shift: true, ..ALT }, eight),
		"\u{c1}".as_bytes()
	);
	assert_eq!(
		send(Key::Named(NamedKey::Enter), ALT, eight),
		"\u{8d}".as_bytes()
	);
	assert_eq!(
		send(text("é"), ALT, eight),
		"é".as_bytes(),
		"only ASCII has a free bit"
	);
	let ctrl_alt = Modifiers { ctrl: true, ..ALT };
	assert_eq!(send(text("c"), ctrl_alt, eight), "\u{83}".as_bytes());
}

#[test]
fn ignored_meta_sends_the_plain_key() {
	let ignore = with(|k| {
		k.meta_escape = false;
		k.eight_bit_meta = false;
	});
	assert_eq!(send(text("a"), ALT, ignore), b"a");
	assert_eq!(send(Key::Named(NamedKey::Backspace), ALT, ignore), b"\x7f");
	assert_eq!(
		send(Key::Named(NamedKey::ArrowUp), ALT, ignore),
		b"\x1b[1;3A"
	);
}

#[test]
fn alt_sends_escape_alone_is_enough_for_escape() {
	let alt = with(|k| {
		k.meta_escape = false;
		k.alt_escape = true;
	});
	assert_eq!(send(text("a"), ALT, alt), b"\x1ba");
}

#[test]
fn meta_modes_do_not_touch_the_kitty_protocol_or_modify_other_keys() {
	let mut eight = with(|k| k.meta_escape = false);
	eight.kitty = true;
	let kitty = KeyModes {
		kitty: true,
		..KeyModes::default()
	};
	assert_eq!(send(text("a"), ALT, eight), send(text("a"), ALT, kitty));
	let mut other = with(|k| k.meta_escape = false);
	other.options.modify[4] = 2;
	assert_eq!(send(text("a"), ALT, other), b"\x1b[27;3;97~");
}

#[test]
fn ignoring_numlock_sends_ss3_for_keypad_digits() {
	let modes = KeyModes {
		application_keypad: true,
		..with(|k| k.numlock = false)
	};
	let rows = [
		("0", "\x1bOp"),
		("1", "\x1bOq"),
		("5", "\x1bOu"),
		("9", "\x1bOy"),
		(".", "\x1bOn"),
		("+", "\x1bOk"),
	];
	for (key, expected) in rows {
		let got = encode_keypad(&text(key), NONE, modes).unwrap();
		assert_eq!(got, expected.as_bytes(), "keypad {key}");
	}
	let got = encode_keypad(&text("7"), SHIFT, modes).unwrap();
	assert_eq!(got, b"\x1bO2w");
}

#[test]
fn numlock_digits_still_type_in_numeric_mode_or_with_numlock_honored() {
	let ignoring = with(|k| k.numlock = false);
	assert_eq!(encode_keypad(&text("5"), NONE, ignoring), None, "DECKPNM");
	let honored = KeyModes {
		application_keypad: true,
		..KeyModes::default()
	};
	assert_eq!(encode_keypad(&text("5"), NONE, honored), None);
}

#[test]
fn the_kitty_protocol_keeps_its_own_delete() {
	let modes = KeyModes {
		kitty: true,
		..with(|k| k.delete_is_del = true)
	};
	assert_eq!(send(Key::Named(NamedKey::Delete), NONE, modes), b"\x1b[3~");
}

#[test]
fn eight_bit_meta_covers_space_escape_and_delete() {
	let eight = with(|k| k.meta_escape = false);
	assert_eq!(
		send(Key::Named(NamedKey::Space), ALT, eight),
		"\u{a0}".as_bytes()
	);
	assert_eq!(
		send(Key::Named(NamedKey::Escape), ALT, eight),
		"\u{9b}".as_bytes()
	);
	assert_eq!(
		send(Key::Named(NamedKey::Backspace), ALT, eight),
		"\u{ff}".as_bytes()
	);
	let bs = with(|k| {
		k.meta_escape = false;
		k.backarrow_bs = true;
	});
	assert_eq!(
		send(Key::Named(NamedKey::Backspace), ALT, bs),
		"\u{88}".as_bytes()
	);
}
