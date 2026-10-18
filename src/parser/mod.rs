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
mod scan;
mod sgr;

use crate::term::Term;
use vte::{Params, Perform};

/// Stateful parser for one PTY stream. Keep it beside `Term` in a session so
/// escape sequences and UTF-8 codepoints split across reads remain intact.
pub(crate) struct Parser {
	inner: vte::Parser,
	/// An incomplete UTF-8 sequence from the end of the previous read.
	carry: [u8; 4],
	carry_len: usize,
	/// True only when vte is certainly in its ground state, which allows the
	/// SIMD fast path. Cleared whenever bytes go through vte, and set again
	/// by a callback after which vte always returns to ground.
	ground: bool,
}

impl Parser {
	pub(crate) fn new() -> Self {
		Self {
			inner: vte::Parser::new(),
			carry: [0; 4],
			carry_len: 0,
			ground: true,
		}
	}

	/// Feeds one PTY read to the terminal.
	///
	/// vte 0.15 mishandles characters split across `advance` calls: its
	/// lookahead can consume (and drop) up to two bytes that follow the split
	/// character. So a trailing incomplete sequence is held back and completed
	/// from the next read; vte only ever sees whole characters.
	pub(crate) fn advance(&mut self, term: &mut Term, mut bytes: &[u8]) {
		if self.carry_len > 0 {
			let needed = utf8_len(self.carry[0]) - self.carry_len;
			let take = needed.min(bytes.len());
			self.carry[self.carry_len..self.carry_len + take].copy_from_slice(&bytes[..take]);
			self.carry_len += take;
			bytes = &bytes[take..];
			if take < needed {
				return;
			}
			let carry = self.carry;
			self.feed(term, &carry[..self.carry_len]);
			self.carry_len = 0;
		}
		let complete = complete_prefix(bytes);
		self.feed(term, &bytes[..complete]);
		let tail = &bytes[complete..];
		self.carry[..tail.len()].copy_from_slice(tail);
		self.carry_len = tail.len();
	}
}

/// Collects runs of printable ASCII from vte's per-character `print` calls
/// and writes each run with [`Term::print_ascii`]; every other callback
/// flushes the run first, so ordering is unchanged.
struct Batch<'a> {
	term: &'a mut Term,
	run: [u8; 1024],
	len: usize,
	/// Set by callbacks after which vte is always back in ground state.
	returned_to_ground: bool,
}

impl<'a> Batch<'a> {
	fn new(term: &'a mut Term) -> Self {
		Self {
			term,
			run: [0; 1024],
			len: 0,
			returned_to_ground: false,
		}
	}

	/// Handles printable ASCII and C0 controls (other than ESC) directly, as
	/// vte would in ground state. Returns the bytes from the first ESC or
	/// non-ASCII byte onwards.
	#[inline]
	fn ground_fast_path<'b>(&mut self, mut bytes: &'b [u8]) -> &'b [u8] {
		self.flush();
		let term = self.term.pending();
		loop {
			let printable = scan::printable_prefix(bytes);
			if printable > 0 {
				term.print_ascii(&bytes[..printable]);
				bytes = &bytes[printable..];
			}
			match bytes.first() {
				Some(&byte) if byte < 0x20 && byte != 0x1b => {
					term.apply_execute(byte);
					bytes = &bytes[1..];
				}
				_ => return bytes,
			}
		}
	}

	#[inline]
	fn flush(&mut self) {
		if self.len > 0 {
			self.term.pending().print_ascii(&self.run[..self.len]);
			self.len = 0;
		}
	}
}

impl Perform for Batch<'_> {
	#[inline(always)]
	fn print(&mut self, c: char) {
		// vte only prints characters >= U+0020; DEL takes the general path.
		if c < '\x7f' {
			if self.len == self.run.len() {
				self.flush();
			}
			self.run[self.len] = c as u8;
			self.len += 1;
		} else {
			self.flush();
			self.term.print(c);
		}
	}
	fn execute(&mut self, byte: u8) {
		self.flush();
		self.term.execute(byte);
		// CAN and SUB abort any sequence and return vte to ground.
		self.returned_to_ground |= matches!(byte, 0x18 | 0x1a);
	}
	fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
		self.flush();
		self.term.hook(params, intermediates, ignore, action);
	}
	fn put(&mut self, byte: u8) {
		self.term.put(byte);
	}
	fn unhook(&mut self) {
		self.term.unhook();
	}
	fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
		self.flush();
		self.term.osc_dispatch(params, bell_terminated);
		// An ESC-terminated OSC leaves vte in its escape state until `\\`.
		self.returned_to_ground |= bell_terminated;
	}
	fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
		self.flush();
		self
			.term
			.csi_dispatch(params, intermediates, ignore, action);
		self.returned_to_ground = true;
	}
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self.flush();
		self.term.esc_dispatch(intermediates, ignore, byte);
		self.returned_to_ground = true;
	}

	fn terminated(&self) -> bool {
		self.returned_to_ground
	}
}

impl Parser {
	/// Processes whole characters, alternating between the fast path (while
	/// vte is known to be in ground state) and vte itself.
	fn feed(&mut self, term: &mut Term, mut bytes: &[u8]) {
		let mut batch = Batch::new(term);
		while !bytes.is_empty() {
			if self.ground {
				bytes = batch.ground_fast_path(bytes);
				if bytes.is_empty() {
					break;
				}
				// ESC or a non-ASCII byte: let vte decide what it means.
				self.ground = false;
			}
			batch.returned_to_ground = false;
			let used = self.inner.advance_until_terminated(&mut batch, bytes);
			bytes = &bytes[used..];
			self.ground = batch.returned_to_ground;
		}
		batch.flush();
	}
}

/// Byte length of the UTF-8 sequence that `lead` starts (1 for non-leads).
fn utf8_len(lead: u8) -> usize {
	match lead {
		0xc0..=0xdf => 2,
		0xe0..=0xef => 3,
		0xf0..=0xf7 => 4,
		_ => 1,
	}
}

/// Length of `bytes` without a trailing, incomplete UTF-8 sequence.
fn complete_prefix(bytes: &[u8]) -> usize {
	for start in (bytes.len().saturating_sub(3)..bytes.len()).rev() {
		let byte = bytes[start];
		if byte & 0xc0 == 0x80 {
			continue; // continuation byte: keep looking for its lead
		}
		return if bytes.len() - start < utf8_len(byte) {
			start
		} else {
			bytes.len()
		};
	}
	bytes.len()
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
