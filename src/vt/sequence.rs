//! Escape, CSI and DCS-header states: short sequences, read byte by byte
//! except for the parameters of CSI sequences.
use super::{Handler, MAX_INTERMEDIATES, Parser, State};

impl Parser {
	/// Runs the sequence states until a sequence ends or a string starts;
	/// returns the bytes used.
	pub(super) fn sequence<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let mut index = 0;
		while index < bytes.len() {
			if matches!(self.state, State::CsiEntry | State::CsiParam) {
				index += self.csi_params(handler, &bytes[index..]);
				if index == bytes.len() || !self.in_sequence() {
					return index;
				}
			}
			self.step(handler, bytes[index]);
			index += 1;
			if !self.in_sequence() {
				return index;
			}
		}
		index
	}

	/// The common shape of CSI sequences, parameters and a final byte, in
	/// one tight loop. Stops before any other byte, which [`Parser::step`]
	/// then handles, so the result is the same as stepping byte by byte.
	#[inline(always)]
	pub(super) fn csi_params<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let mut param = self.param;
		for (index, &byte) in bytes.iter().enumerate() {
			match byte {
				b'0'..=b'9' if !self.params.is_full() => {
					param = param
						.saturating_mul(10)
						.saturating_add(u16::from(byte - b'0'));
				}
				b';' if !self.params.is_full() => self.params.push(std::mem::take(&mut param)),
				b':' if !self.params.is_full() => self.params.push_sub(std::mem::take(&mut param)),
				0x40..=0x7e => {
					self.param = param;
					self.final_byte(handler, byte, false);
					return index + 1;
				}
				_ => {
					self.param = param;
					if index > 0 {
						self.state = State::CsiParam;
					}
					return index;
				}
			}
		}
		self.param = param;
		if !bytes.is_empty() {
			self.state = State::CsiParam;
		}
		bytes.len()
	}

	fn in_sequence(&self) -> bool {
		!matches!(
			self.state,
			State::Ground
				| State::Osc
				| State::DcsPassthrough
				| State::DcsIgnore
				| State::Apc
				| State::IgnoredString
		)
	}

	#[inline]
	fn step<H: Handler>(&mut self, handler: &mut H, byte: u8) {
		match byte {
			0x18 | 0x1a => {
				handler.execute(byte);
				self.state = State::Ground;
				return;
			}
			0x1b => return self.enter_escape(),
			// Bytes outside 7-bit ASCII mean nothing inside a sequence.
			0x7f.. => return,
			_ => {}
		}
		let dcs = matches!(
			self.state,
			State::DcsEntry | State::DcsParam | State::DcsIntermediate
		);
		if byte < 0x20 {
			// C0 controls run in the middle of sequences, except in DCS headers.
			if !dcs {
				handler.execute(byte);
			}
			return;
		}
		match self.state {
			State::Escape => self.escape(handler, byte),
			State::EscapeIntermediate => match byte {
				0x20..=0x2f => self.collect(byte),
				_ => self.esc_dispatch(handler, byte),
			},
			State::CsiEntry | State::DcsEntry => match byte {
				0x20..=0x2f => {
					self.collect(byte);
					self.state = if dcs {
						State::DcsIntermediate
					} else {
						State::CsiIntermediate
					};
				}
				b'0'..=b';' => {
					self.param_byte(byte);
					self.state = if dcs {
						State::DcsParam
					} else {
						State::CsiParam
					};
				}
				// Private markers such as `?`.
				b'<'..=b'?' => {
					self.collect(byte);
					self.state = if dcs {
						State::DcsParam
					} else {
						State::CsiParam
					};
				}
				_ => self.final_byte(handler, byte, dcs),
			},
			State::CsiParam | State::DcsParam => match byte {
				0x20..=0x2f => {
					self.collect(byte);
					self.state = if dcs {
						State::DcsIntermediate
					} else {
						State::CsiIntermediate
					};
				}
				b'0'..=b';' => self.param_byte(byte),
				b'<'..=b'?' => {
					self.state = if dcs {
						State::DcsIgnore
					} else {
						State::CsiIgnore
					}
				}
				_ => self.final_byte(handler, byte, dcs),
			},
			State::CsiIntermediate | State::DcsIntermediate => match byte {
				0x20..=0x2f => self.collect(byte),
				0x30..=0x3f => {
					self.state = if dcs {
						State::DcsIgnore
					} else {
						State::CsiIgnore
					}
				}
				_ => self.final_byte(handler, byte, dcs),
			},
			State::CsiIgnore => {
				if byte >= 0x40 {
					self.state = State::Ground;
				}
			}
			_ => unreachable!("{:?} is not a sequence state", self.state),
		}
	}

	fn escape<H: Handler>(&mut self, handler: &mut H, byte: u8) {
		match byte {
			0x20..=0x2f => {
				self.collect(byte);
				self.state = State::EscapeIntermediate;
			}
			b'[' => self.enter_csi(),
			b'P' => {
				self.enter_csi();
				self.state = State::DcsEntry;
			}
			b']' => {
				self.osc.clear();
				self.osc_fields = 0;
				self.osc_overflow = false;
				self.state = State::Osc;
			}
			b'_' => {
				handler.apc_start();
				self.state = State::Apc;
			}
			b'X' | b'^' => self.state = State::IgnoredString,
			_ => self.esc_dispatch(handler, byte),
		}
	}

	pub(super) fn enter_csi(&mut self) {
		self.params.clear();
		self.param = 0;
		self.state = State::CsiEntry;
	}

	fn esc_dispatch<H: Handler>(&mut self, handler: &mut H, byte: u8) {
		handler.esc_dispatch(
			&self.intermediates[..self.intermediate_len],
			self.ignore,
			byte,
		);
		self.state = State::Ground;
	}

	fn collect(&mut self, byte: u8) {
		if self.intermediate_len == MAX_INTERMEDIATES {
			self.ignore = true;
		} else {
			self.intermediates[self.intermediate_len] = byte;
			self.intermediate_len += 1;
		}
	}

	/// A digit, `:` (subparameter) or `;` (parameter) separator.
	#[inline]
	fn param_byte(&mut self, byte: u8) {
		if self.params.is_full() {
			self.ignore = true;
			return;
		}
		match byte {
			b';' => self.params.push(std::mem::take(&mut self.param)),
			b':' => self.params.push_sub(std::mem::take(&mut self.param)),
			digit => {
				self.param = self
					.param
					.saturating_mul(10)
					.saturating_add(u16::from(digit - b'0'))
			}
		}
	}

	/// Ends a CSI sequence, or a DCS header (the payload follows).
	fn final_byte<H: Handler>(&mut self, handler: &mut H, action: u8, dcs: bool) {
		if self.params.is_full() {
			self.ignore = true;
			self.params.close();
		} else {
			self.params.push(self.param);
		}
		let intermediates = &self.intermediates[..self.intermediate_len];
		if dcs {
			handler.hook(&self.params, intermediates, self.ignore, action);
			self.state = State::DcsPassthrough;
		} else {
			handler.csi_dispatch(&self.params, intermediates, self.ignore, action);
			self.state = State::Ground;
		}
	}
}
