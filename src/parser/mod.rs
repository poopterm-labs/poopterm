//! Escape-sequence handling using vte's incremental state machine.
//!
//! `vte` tokenizes the byte stream; the `Perform` impl here routes each token to
//! the submodule that applies it to a [`Term`]. While synchronized output
//! (`?2026`) is active, everything except the mode switch itself goes to the
//! pending snapshot returned by [`Term::pending`].
mod csi;
mod esc;
mod osc;
mod print;
mod sgr;

use crate::term::Term;
use vte::{Params, Perform};

/// Stateful parser for one PTY stream. Keep it beside `Term` in a session so
/// escape sequences and UTF-8 codepoints split across reads remain intact.
pub(crate) struct Parser {
	inner: vte::Parser,
}

impl Parser {
	pub(crate) fn new() -> Self {
		Self {
			inner: vte::Parser::new(),
		}
	}

	pub(crate) fn advance(&mut self, term: &mut Term, bytes: &[u8]) {
		self.inner.advance(term, bytes);
	}
}

impl Perform for Term {
	#[inline(always)]
	fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
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
	#[inline(always)]
	fn put(&mut self, byte: u8) {
		if let Some(transfer) = &mut self.pending().sixel_transfer {
			transfer.push(byte);
		}
	}
	#[inline(always)]
	fn unhook(&mut self) {
		self.pending().finish_sixel();
	}

	#[inline(always)]
	fn print(&mut self, c: char) {
		self.pending().apply_print(c);
	}
	#[inline(always)]
	fn execute(&mut self, byte: u8) {
		self.pending().apply_execute(byte);
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
	}
	#[inline(always)]
	fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
		if ignore {
			return;
		}
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
		self
			.pending()
			.apply_csi_dispatch(params, intermediates, false, action);
		if let Some(pending) = &mut self.synchronized {
			self.responses.append(&mut pending.responses);
		}
	}
}
