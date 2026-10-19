//! Applies escape sequences to a [`Term`].
//!
//! [`crate::vt`] tokenizes the byte stream; the [`Handler`] impl here routes
//! each event to the submodule that applies it. While synchronized output
//! (`?2026`) is active, everything except the mode switch itself goes to the
//! pending snapshot returned by [`Term::pending`].
mod color_spec;
mod csi;
mod erase;
mod esc;
mod osc;
mod osc_clipboard;
mod osc_color;
mod osc_iterm;
mod osc_notify;
mod osc_pointer;
mod osc_shell;
mod osc_status;
mod osc_tab;
mod params;
mod print;
mod sgr;
mod utf8;
mod window_ops;

use crate::{
	term::Term,
	vt::{self, Handler},
};
#[cfg(test)]
pub(crate) use color_spec::encode as encode_color;
use params::ParamList;

/// Stateful parser for one PTY stream. Keep it beside `Term` in a session so
/// escape sequences and UTF-8 characters split across reads remain intact.
#[derive(Default)]
pub(crate) struct Parser(vt::Parser);

impl Parser {
	pub(crate) fn new() -> Self {
		Self::default()
	}

	/// Feeds one PTY read to the terminal.
	#[inline]
	pub(crate) fn advance(&mut self, term: &mut Term, bytes: &[u8]) {
		self.0.advance(term, bytes);
	}
}

impl Handler for Term {
	#[inline(always)]
	fn print_ascii(&mut self, text: &[u8]) {
		let term = self.pending();
		if term.iterm.capture.is_some() {
			// Printable ASCII is UTF-8.
			term
				.iterm
				.capture(std::str::from_utf8(text).unwrap_or_default());
		}
		term.print_ascii(text);
	}
	#[inline(always)]
	fn print(&mut self, text: &str) {
		let term = self.pending();
		if term.iterm.capture.is_some() {
			term.iterm.capture(text);
		}
		term.print_text(text);
	}
	#[inline(always)]
	fn execute(&mut self, byte: u8) {
		let term = self.pending();
		if term.iterm.capture.is_some() && matches!(byte, b'\n' | 0x0b | 0x0c) {
			term.iterm.capture("\n");
		}
		term.apply_execute(byte);
	}
	#[inline(always)]
	fn csi_dispatch(&mut self, params: &vt::Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.dispatch_csi(params, intermediates, ignore, action as char);
	}
	#[inline(always)]
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self
			.pending()
			.apply_esc_dispatch(intermediates, ignore, byte);
	}
	#[inline(always)]
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.pending().apply_osc_dispatch(params, bell);
		// Replies are not held back by synchronized output.
		if let Some(pending) = &mut self.synchronized {
			self.responses.append(&mut pending.responses);
		}
	}
	#[inline(always)]
	fn hook(&mut self, params: &vt::Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.dispatch_hook(params, intermediates, ignore, action as char);
	}
	#[inline(always)]
	fn put(&mut self, bytes: &[u8]) {
		if let Some(transfer) = &mut self.pending().sixel_transfer {
			transfer.extend(bytes);
		}
	}
	#[inline(always)]
	fn unhook(&mut self) {
		self.pending().finish_sixel();
	}
	// The receiver belongs to the live terminal so a transfer spans
	// synchronized output; finished commands apply to the pending state.
	fn apc_start(&mut self) {
		self.kitty_receiver.start();
	}
	fn apc_put(&mut self, bytes: &[u8]) {
		self.kitty_receiver.put(bytes);
	}
	fn apc_end(&mut self, complete: bool) {
		if let Some(transmission) = self.kitty_receiver.end(complete)
			&& let Some(reply) = self.pending().apply_kitty(transmission)
		{
			// Replies are not held back by synchronized output.
			self.responses.push(reply);
		}
	}
}

impl Term {
	fn dispatch_hook(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		let target = self.pending();
		target.sixel_transfer = if !ignore && intermediates.is_empty() && action == 'q' {
			let mut values = params.iter().map(|p| p.first().copied().unwrap_or(0));
			Some(crate::sixel::Transfer::new(
				values.next().unwrap_or(0),
				values.next().unwrap_or(0),
			))
		} else {
			None
		};
	}

	fn dispatch_csi(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		if ignore {
			return;
		}
		// Most sequences have no intermediates; skip the checks below for them.
		if !intermediates.is_empty() {
			self.dispatch_mode_sequences(params, intermediates, action);
			if intermediates == b"?$" && action == 'p' {
				return;
			}
		}
		self
			.pending()
			.apply_csi_dispatch(params, intermediates, false, action);
		if let Some(pending) = &mut self.synchronized {
			self.responses.append(&mut pending.responses);
		}
	}

	/// DECRQM (`CSI ? Ps $ p`) and synchronized output (`CSI ? 2026 h/l`),
	/// which act on the terminal rather than on the pending snapshot.
	fn dispatch_mode_sequences(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		action: char,
	) {
		if intermediates == b"?$" && action == 'p' {
			for param in params.iter() {
				let mode = param[0];
				let state = if mode == 2026 {
					Some(self.synchronized.is_some())
				} else {
					self.pending().private_mode(mode)
				};
				let status = match state {
					Some(true) => 1,
					Some(false) => 2,
					None => 0,
				};
				self
					.responses
					.push(format!("\x1b[?{mode};{status}$y").into_bytes());
			}
			return;
		}
		if intermediates == b"?"
			&& matches!(action, 'h' | 'l')
			&& params.iter().any(|p| p.first() == Some(&2026))
		{
			if action == 'h' {
				self.begin_sync();
			} else {
				self.finish_sync();
			}
		}
	}
}

/// vte driving a [`Term`] one character at a time: the reference that the
/// batched paths are compared against in tests.
#[cfg(test)]
pub(crate) struct PerCharacter<'a>(pub &'a mut Term);

#[cfg(test)]
impl vte::Perform for PerCharacter<'_> {
	fn print(&mut self, c: char) {
		self.0.pending().apply_print(c);
	}
	fn execute(&mut self, byte: u8) {
		self.0.execute(byte);
	}
	fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
		self.0.dispatch_hook(params, intermediates, ignore, action);
	}
	fn put(&mut self, byte: u8) {
		self.0.put(&[byte]);
	}
	fn unhook(&mut self) {
		self.0.unhook();
	}
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.0.osc_dispatch(params, bell);
	}
	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		self.0.dispatch_csi(params, intermediates, ignore, action);
	}
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self.0.esc_dispatch(intermediates, ignore, byte);
	}
}
