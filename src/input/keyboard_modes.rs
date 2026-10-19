//! DEC private modes that change what keys send: DECARM (?8), DECBKM (?67)
//! and xterm's Meta (?1034, ?1036, ?1039), NumLock (?1035) and Delete
//! (?1037) modes.

/// The keyboard modes a program set; each field is one private mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyboardModes {
	/// ?8 DECARM: held keys repeat.
	pub auto_repeat: bool,
	/// ?67 DECBKM: Backspace sends BS instead of DEL.
	pub backarrow_bs: bool,
	/// ?1034: Meta sets the eighth bit when it does not send ESC.
	pub eight_bit_meta: bool,
	/// ?1035: NumLock keeps keypad digits digits in application mode.
	pub numlock: bool,
	/// ?1036: Meta sends ESC first.
	pub meta_escape: bool,
	/// ?1037: Delete sends DEL instead of `CSI 3 ~`.
	pub delete_is_del: bool,
	/// ?1039: Alt sends ESC first.
	pub alt_escape: bool,
}

impl Default for KeyboardModes {
	fn default() -> Self {
		Self {
			auto_repeat: true,
			backarrow_bs: false,
			eight_bit_meta: true,
			numlock: true,
			meta_escape: true,
			delete_is_del: false,
			alt_escape: false,
		}
	}
}

/// What Alt (xterm's Meta) does to a key's bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Meta {
	Escape,
	EightBit,
	Ignore,
}

impl KeyboardModes {
	/// The flag behind private mode `mode`, if it is a keyboard mode.
	pub(crate) fn mode(&mut self, mode: u16) -> Option<&mut bool> {
		Some(match mode {
			8 => &mut self.auto_repeat,
			67 => &mut self.backarrow_bs,
			1034 => &mut self.eight_bit_meta,
			1035 => &mut self.numlock,
			1036 => &mut self.meta_escape,
			1037 => &mut self.delete_is_del,
			1039 => &mut self.alt_escape,
			_ => return None,
		})
	}

	/// Alt is the Meta key here, so either escape mode sends ESC, which
	/// takes precedence over the eighth bit.
	pub(crate) fn meta(&self) -> Meta {
		match (self.meta_escape || self.alt_escape, self.eight_bit_meta) {
			(true, _) => Meta::Escape,
			(false, true) => Meta::EightBit,
			(false, false) => Meta::Ignore,
		}
	}
}
