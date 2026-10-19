//! xterm's key modifier options: XTMODKEYS (`CSI > Pp ; Pv m`), their
//! formats (XTFMTKEYS, `CSI > Pp ; Pv f`), and the keys they change.
//!
//! Resources are numbered as in the control sequences: 0 keyboard, 1 cursor
//! keys, 2 function keys, 3 keypad keys, 4 other keys, 6 modifier keys and
//! 7 special keys.
use super::keys::Modifiers;
use winit::keyboard::{Key, NamedKey};

/// The modifier options a program set; a modify value of -1 is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyOptions {
	pub modify: [i8; 8],
	pub format: [u8; 8],
	/// Modifier bits factored out of modifyOtherKeys sequences (the
	/// subparameter of `CSI > 4 : mask ; Pv m`).
	pub other_mask: u8,
}

/// xterm's initial values: modifyCursorKeys and modifyFunctionKeys are 2.
const INITIAL_MODIFY: [i8; 8] = [0, 2, 2, 0, 0, 0, 0, 0];

impl Default for KeyOptions {
	fn default() -> Self {
		Self {
			modify: INITIAL_MODIFY,
			format: [0; 8],
			other_mask: 0,
		}
	}
}

pub(crate) const CURSOR_KEYS: usize = 1;
pub(crate) const FUNCTION_KEYS: usize = 2;
pub(crate) const KEYPAD_KEYS: usize = 3;
const OTHER_KEYS: usize = 4;

const SHIFT: u8 = 1;
const ALT: u8 = 2;
const CTRL: u8 = 4;
const SUPER: u8 = 8;

/// Resources the sequences may name; 5 is reserved.
fn resource(index: u16) -> Option<usize> {
	matches!(index, 0..=4 | 6 | 7).then_some(usize::from(index))
}

impl KeyOptions {
	/// XTMODKEYS for one resource: `value` sets it, `None` restores its
	/// initial value. `mask` is the subparameter of the resource number.
	pub(crate) fn set_modify(&mut self, index: u16, mask: Option<u16>, value: Option<u16>) {
		let Some(index) = resource(index) else {
			return;
		};
		self.modify[index] = value.map_or(INITIAL_MODIFY[index], |v| v.min(i8::MAX as u16) as i8);
		if index == OTHER_KEYS {
			self.other_mask = mask.map_or(0, |mask| (mask & 0xf) as u8);
		}
	}

	/// XTMODKEYS without parameters: every resource to its initial value.
	pub(crate) fn reset_modify(&mut self) {
		self.modify = INITIAL_MODIFY;
		self.other_mask = 0;
	}

	/// `CSI > Ps n`: disables a resource, modifyFunctionKeys by default.
	pub(crate) fn disable(&mut self, index: Option<u16>) {
		if let Some(index) = resource(index.unwrap_or(FUNCTION_KEYS as u16)) {
			self.modify[index] = -1;
		}
	}

	/// XTFMTKEYS: `None` restores the initial format (0).
	pub(crate) fn set_format(&mut self, index: Option<u16>, value: Option<u16>) {
		match index.and_then(resource) {
			Some(index) => self.format[index] = value.map_or(0, |v| v.min(u8::MAX as u16) as u8),
			None if index.is_none() => self.format = [0; 8],
			None => {}
		}
	}

	/// XTQMODKEYS's answer, in XTMODKEYS form; a disabled resource has no
	/// value.
	pub(crate) fn modify_reply(&self, index: u16) -> Option<String> {
		let value = self.modify[resource(index)?];
		Some(if value < 0 {
			format!("\x1b[>{index}m")
		} else {
			format!("\x1b[>{index};{value}m")
		})
	}

	/// XTQFMTKEYS's answer, as the XTFMTKEYS sequence restoring the value.
	pub(crate) fn format_reply(&self, index: u16) -> Option<String> {
		let value = self.format[resource(index)?];
		Some(format!("\x1b[>{index};{value}f"))
	}
}

/// A cursor, function or keypad key with xterm's modifier parameter
/// `param` (1 when unmodified), at modify level `level`. `number` is the
/// `CSI n ~` key number, if the key has one; otherwise `last` is the final
/// of an `SS3` or `CSI` form.
pub(crate) fn with_modifiers(
	level: i8,
	number: Option<u16>,
	param: u8,
	last: char,
	unmodified: Vec<u8>,
) -> Vec<u8> {
	if param <= 1 || level < 0 {
		return unmodified;
	}
	match (number, level) {
		(Some(n), 0..=2) => format!("\x1b[{n};{param}{last}"),
		(Some(n), _) => format!("\x1b[>{n};{param}{last}"),
		(None, 0) => format!("\x1bO{param}{last}"),
		(None, 1) => format!("\x1b[{param}{last}"),
		(None, 2) => format!("\x1b[1;{param}{last}"),
		(None, _) => format!("\x1b[>1;{param}{last}"),
	}
	.into_bytes()
}

/// Encodes an ordinary key as modifyOtherKeys asks, or returns `None` when
/// the key keeps its usual encoding.
pub(crate) fn encode_other_key(key: &Key, mods: Modifiers, options: KeyOptions) -> Option<Vec<u8>> {
	let level = options.modify[OTHER_KEYS];
	if level <= 0 {
		return None;
	}
	let (code, character) = match key {
		Key::Character(text) => {
			let mut chars = text.chars();
			let c = chars.next()?;
			if chars.next().is_some() {
				return None;
			}
			(u32::from(c), Some(c))
		}
		Key::Named(NamedKey::Enter) => (13, None),
		Key::Named(NamedKey::Tab) => (9, None),
		Key::Named(NamedKey::Backspace) => (127, None),
		Key::Named(NamedKey::Escape) => (27, None),
		Key::Named(NamedKey::Space) => (32, Some(' ')),
		_ => return None,
	};
	let bits = (u8::from(mods.shift) * SHIFT
		+ u8::from(mods.alt) * ALT
		+ u8::from(mods.ctrl) * CTRL
		+ u8::from(mods.super_key) * SUPER)
		& !options.other_mask;
	if level < 3 {
		// A shifted character already says Shift ("A" rather than "a").
		if bits == 0 || (bits == SHIFT && character.is_some()) {
			return None;
		}
		// Level 1 keeps keys that have a usual meaning with Shift and Ctrl.
		if level == 1 && bits & (ALT | SUPER) == 0 {
			let usual = match (key, character) {
				(Key::Named(NamedKey::Tab), _) => bits == SHIFT,
				(_, Some(c)) => bits & CTRL == 0 || control_of(c),
				_ => false,
			};
			if usual {
				return None;
			}
		}
	}
	let param = 1 + bits;
	Some(if options.format[OTHER_KEYS] == 1 {
		format!("\x1b[{code};{param}u").into_bytes()
	} else {
		format!("\x1b[27;{param};{code}~").into_bytes()
	})
}

/// Whether Ctrl with `c` produces a C0 control, as in xterm's mapping.
fn control_of(c: char) -> bool {
	c.is_ascii_alphabetic()
		|| matches!(
			c,
			' ' | '@' | '[' | '\\' | ']' | '^' | '_' | '?' | '2'..='8' | '/'
		)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn level(value: i8) -> KeyOptions {
		let mut options = KeyOptions::default();
		options.modify[OTHER_KEYS] = value;
		options
	}

	fn mods(bits: u8) -> Modifiers {
		Modifiers {
			shift: bits & SHIFT != 0,
			alt: bits & ALT != 0,
			ctrl: bits & CTRL != 0,
			super_key: bits & SUPER != 0,
		}
	}

	fn text(s: &str) -> Key {
		Key::Character(s.into())
	}

	fn other(key: Key, bits: u8, options: KeyOptions) -> Option<String> {
		encode_other_key(&key, mods(bits), options).map(|b| String::from_utf8(b).unwrap())
	}

	#[test]
	fn initial_options_match_xterm() {
		let options = KeyOptions::default();
		assert_eq!(options.modify, [0, 2, 2, 0, 0, 0, 0, 0]);
		assert_eq!(options.format, [0; 8]);
		assert_eq!(
			other(text("a"), CTRL, options),
			None,
			"modifyOtherKeys starts off"
		);
	}

	#[test]
	fn level_two_encodes_every_modified_key() {
		let options = level(2);
		assert_eq!(other(text("a"), CTRL, options).unwrap(), "\x1b[27;5;97~");
		assert_eq!(
			other(text("A"), CTRL | SHIFT, options).unwrap(),
			"\x1b[27;6;65~"
		);
		assert_eq!(other(text("x"), ALT, options).unwrap(), "\x1b[27;3;120~");
		assert_eq!(
			other(Key::Named(NamedKey::Tab), SHIFT, options).unwrap(),
			"\x1b[27;2;9~"
		);
		assert_eq!(
			other(Key::Named(NamedKey::Enter), CTRL, options).unwrap(),
			"\x1b[27;5;13~"
		);
		assert_eq!(
			other(Key::Named(NamedKey::Space), CTRL | ALT, options).unwrap(),
			"\x1b[27;7;32~"
		);
		assert_eq!(other(text("1"), SUPER, options).unwrap(), "\x1b[27;9;49~");
	}

	#[test]
	fn shifted_characters_and_unmodified_keys_stay_plain_below_level_three() {
		for value in [1, 2] {
			let options = level(value);
			assert_eq!(other(text("A"), SHIFT, options), None);
			assert_eq!(other(text("!"), SHIFT, options), None);
			assert_eq!(other(text("a"), 0, options), None);
			assert_eq!(other(Key::Named(NamedKey::Enter), 0, options), None);
		}
	}

	#[test]
	fn level_one_keeps_the_usual_shift_and_ctrl_keys() {
		let options = level(1);
		assert_eq!(other(text("a"), CTRL, options), None, "Ctrl+A is ^A");
		assert_eq!(other(text("["), CTRL, options), None, "Ctrl+[ is ESC");
		assert_eq!(
			other(Key::Named(NamedKey::Tab), SHIFT, options),
			None,
			"back tab"
		);
		assert_eq!(other(text("1"), CTRL, options).unwrap(), "\x1b[27;5;49~");
		assert_eq!(other(text(";"), CTRL, options).unwrap(), "\x1b[27;5;59~");
		assert_eq!(other(text("x"), ALT, options).unwrap(), "\x1b[27;3;120~");
		assert_eq!(
			other(Key::Named(NamedKey::Tab), ALT, options).unwrap(),
			"\x1b[27;3;9~",
			"xterm's own example"
		);
	}

	#[test]
	fn level_three_encodes_unmodified_keys_too() {
		let options = level(3);
		assert_eq!(
			other(Key::Named(NamedKey::Space), 0, options).unwrap(),
			"\x1b[27;1;32~"
		);
		assert_eq!(other(text("a"), 0, options).unwrap(), "\x1b[27;1;97~");
		assert_eq!(other(text("A"), SHIFT, options).unwrap(), "\x1b[27;2;65~");
	}

	#[test]
	fn format_one_puts_the_code_first_and_ends_with_u() {
		let mut options = level(1);
		options.format[OTHER_KEYS] = 1;
		assert_eq!(
			other(Key::Named(NamedKey::Tab), ALT, options).unwrap(),
			"\x1b[9;3u"
		);
	}

	#[test]
	fn the_mask_factors_modifiers_out() {
		let mut options = level(2);
		options.other_mask = SHIFT;
		assert_eq!(
			other(text("A"), CTRL | SHIFT, options).unwrap(),
			"\x1b[27;5;65~"
		);
		assert_eq!(
			other(Key::Named(NamedKey::Tab), SHIFT, options),
			None,
			"nothing is left to report"
		);
	}

	#[test]
	fn keys_without_a_character_are_not_other_keys() {
		let options = level(2);
		assert_eq!(other(Key::Named(NamedKey::ArrowUp), CTRL, options), None);
		assert_eq!(other(Key::Named(NamedKey::F1), CTRL, options), None);
		assert_eq!(other(text("ab"), CTRL, options), None, "composed text");
	}

	#[test]
	fn disabling_and_negative_levels_turn_other_keys_off() {
		let mut options = level(2);
		options.disable(Some(4));
		assert_eq!(options.modify[OTHER_KEYS], -1);
		assert_eq!(other(text("a"), CTRL, options), None);
	}

	#[test]
	fn xtmodkeys_sets_resets_and_ignores_unknown_resources() {
		let mut options = KeyOptions::default();
		options.set_modify(4, None, Some(2));
		options.set_modify(1, None, Some(3));
		options.set_modify(5, None, Some(1));
		options.set_modify(9, None, Some(1));
		assert_eq!(options.modify, [0, 3, 2, 0, 2, 0, 0, 0]);
		options.set_modify(1, None, None);
		assert_eq!(options.modify[CURSOR_KEYS], 2, "back to the initial value");
		options.reset_modify();
		assert_eq!(options, KeyOptions::default());
	}

	#[test]
	fn xtmodkeys_mask_applies_only_to_other_keys() {
		let mut options = KeyOptions::default();
		options.set_modify(1, Some(1), Some(2));
		assert_eq!(options.other_mask, 0);
		options.set_modify(4, Some(0x13), Some(2));
		assert_eq!(options.other_mask, 3, "only the four modifier bits");
		options.set_modify(4, None, Some(2));
		assert_eq!(options.other_mask, 0);
	}

	#[test]
	fn huge_values_are_clamped() {
		let mut options = KeyOptions::default();
		options.set_modify(4, None, Some(65535));
		options.set_format(Some(4), Some(65535));
		assert_eq!((options.modify[4], options.format[4]), (127, 255));
	}

	#[test]
	fn disable_defaults_to_function_keys() {
		let mut options = KeyOptions::default();
		options.disable(None);
		assert_eq!(options.modify[FUNCTION_KEYS], -1);
		options.disable(Some(5));
		assert_eq!(options.modify, [0, 2, -1, 0, 0, 0, 0, 0]);
	}

	#[test]
	fn xtfmtkeys_sets_one_or_resets_all() {
		let mut options = KeyOptions::default();
		options.set_format(Some(4), Some(1));
		options.set_format(Some(1), Some(1));
		options.set_format(Some(1), None);
		assert_eq!(options.format, [0, 0, 0, 0, 1, 0, 0, 0]);
		options.set_format(Some(5), Some(1));
		assert_eq!(options.format[5], 0, "5 is reserved");
		options.set_format(None, None);
		assert_eq!(options.format, [0; 8]);
	}

	#[test]
	fn queries_answer_with_the_sequence_restoring_the_value() {
		let mut options = KeyOptions::default();
		options.set_modify(4, None, Some(2));
		options.disable(Some(2));
		options.set_format(Some(4), Some(1));
		assert_eq!(options.modify_reply(4).unwrap(), "\x1b[>4;2m");
		assert_eq!(options.modify_reply(1).unwrap(), "\x1b[>1;2m");
		assert_eq!(options.modify_reply(2).unwrap(), "\x1b[>2m");
		assert_eq!(options.format_reply(4).unwrap(), "\x1b[>4;1f");
		assert_eq!(options.modify_reply(5), None);
		assert_eq!(options.format_reply(8), None);
	}

	#[test]
	fn modifier_levels_shape_cursor_and_ss3_keys() {
		let up =
			|level| String::from_utf8(with_modifiers(level, None, 5, 'A', b"\x1b[A".to_vec())).unwrap();
		assert_eq!(up(-1), "\x1b[A", "disabled drops the modifiers");
		assert_eq!(up(0), "\x1bO5A");
		assert_eq!(up(1), "\x1b[5A");
		assert_eq!(up(2), "\x1b[1;5A");
		assert_eq!(up(3), "\x1b[>1;5A");
	}

	#[test]
	fn modifier_levels_shape_numbered_keys() {
		let f5 = |level| {
			String::from_utf8(with_modifiers(
				level,
				Some(15),
				2,
				'~',
				b"\x1b[15~".to_vec(),
			))
			.unwrap()
		};
		assert_eq!(f5(-1), "\x1b[15~");
		assert_eq!(f5(0), "\x1b[15;2~");
		assert_eq!(f5(2), "\x1b[15;2~");
		assert_eq!(f5(3), "\x1b[>15;2~");
	}

	#[test]
	fn unmodified_keys_keep_their_sequence_at_every_level() {
		for level in -1..=3 {
			assert_eq!(
				with_modifiers(level, None, 1, 'A', b"\x1bOA".to_vec()),
				b"\x1bOA"
			);
		}
	}
}
