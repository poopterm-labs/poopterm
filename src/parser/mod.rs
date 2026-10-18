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

#[cfg(test)]
mod bench {
	//! Parser-only throughput, excluding the PTY and rendering. Run with
	//! `cargo test --release -- --ignored --nocapture parser_throughput`.
	use super::Parser;
	use crate::{
		term::Term,
		theme::{Theme, ThemeKind},
	};
	use std::time::Instant;

	fn payload(kind: &str) -> Vec<u8> {
		let mut seed = 0x2545_f491_u32;
		let mut next = move || {
			seed ^= seed << 13;
			seed ^= seed >> 17;
			seed ^= seed << 5;
			seed
		};
		let mut out = Vec::with_capacity(64 << 20);
		while out.len() < 64 << 20 {
			match kind {
				"ascii" => {
					for _ in 0..40 + next() % 80 {
						out.push(0x20 + (next() % 95) as u8);
					}
					out.extend_from_slice(b"\r\n");
				}
				"unicode" => out.extend_from_slice("héllo wörld 日本語 → λ 🙂 naïve ─┼─\r\n".as_bytes()),
				_ => out.extend_from_slice(
					format!(
						"\x1b[{};{}mab\x1b[0m\x1b[38;2;{};{};{}mx",
						30 + next() % 8,
						40 + next() % 8,
						next() % 256,
						next() % 256,
						next() % 256
					)
					.as_bytes(),
				),
			}
		}
		out
	}

	#[test]
	#[ignore = "benchmark; run explicitly in release mode"]
	fn parser_throughput() {
		for kind in ["ascii", "unicode", "csi"] {
			let data = payload(kind);
			let mut term = Term::new(200, 50, Theme::from_kind(ThemeKind::Poop), "/".into());
			let mut parser = Parser::new();
			let start = Instant::now();
			for chunk in data.chunks(1 << 16) {
				parser.advance(&mut term, chunk);
			}
			let rate = data.len() as f64 / start.elapsed().as_secs_f64() / 1e6;
			println!("parser {kind:<8} {rate:8.1} MB/s");
		}
	}
}
