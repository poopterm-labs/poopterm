//! ANSI modes (SM, RM), DECRQM answers, and saving DEC private modes with
//! XTSAVE / XTRESTORE.
use crate::term::Term;

impl Term {
	pub(super) fn set_ansi_mode(&mut self, mode: u16, enabled: bool) {
		match mode {
			2 => self.keyboard_locked = enabled,
			4 => self.insert_mode = enabled,
			// SRM set means no local echo.
			12 => self.local_echo = !enabled,
			20 => self.newline_mode = enabled,
			_ => {}
		}
	}

	/// The state of an ANSI mode for DECRQM, or `None` if unsupported.
	pub(crate) fn ansi_mode(&self, mode: u16) -> Option<bool> {
		Some(match mode {
			2 => self.keyboard_locked,
			4 => self.insert_mode,
			12 => !self.local_echo,
			20 => self.newline_mode,
			_ => return None,
		})
	}

	/// XTSAVE: remembers the current value of each known private mode.
	pub(super) fn save_private_modes(&mut self, modes: impl Iterator<Item = u16>) {
		for mode in modes {
			let Some(value) = self.private_mode(mode) else {
				continue;
			};
			match self
				.saved_modes
				.iter_mut()
				.find(|(saved, _)| *saved == mode)
			{
				Some(entry) => entry.1 = value,
				None => self.saved_modes.push((mode, value)),
			}
		}
	}

	/// XTRESTORE: sets each saved mode back to its saved value. Resets go
	/// first, so a reset mouse mode cannot turn off one restored as set.
	pub(super) fn restore_private_modes(&mut self, modes: impl Iterator<Item = u16>) {
		let mut restored: Vec<(u16, bool)> = modes
			.filter_map(|mode| self.saved_modes.iter().find(|(saved, _)| *saved == mode))
			.copied()
			.collect();
		restored.sort_by_key(|&(_, set)| set);
		for (mode, set) in restored {
			self.apply_csi_dispatch(&[mode][..], b"?", false, if set { 'h' } else { 'l' });
		}
	}
}
