//! String states (OSC, DCS payloads, APC, SOS and PM), scanned in bulk.
use super::{Handler, MAX_OSC_BYTES, MAX_OSC_PARAMS, Parser, State, simd};

impl Parser {
	/// OSC: collects fields until BEL, ESC, CAN or SUB; other C0 controls
	/// are dropped.
	pub(super) fn osc_string<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let mut used = 0;
		loop {
			let run = simd::osc_run(&bytes[used..]);
			self.osc_extend(&bytes[used..used + run]);
			used += run;
			let Some(&byte) = bytes.get(used) else {
				return used;
			};
			used += 1;
			match byte {
				b';' if self.osc_fields < MAX_OSC_PARAMS => {
					self.osc_ends[self.osc_fields] = self.osc.len();
					self.osc_fields += 1;
				}
				0x07 => {
					self.osc_dispatch(handler, true);
					self.state = State::Ground;
					return used;
				}
				0x18 | 0x1a => {
					self.osc_dispatch(handler, false);
					handler.execute(byte);
					self.state = State::Ground;
					return used;
				}
				0x1b => {
					self.osc_dispatch(handler, false);
					self.enter_escape();
					return used;
				}
				_ => {}
			}
		}
	}

	fn osc_extend(&mut self, bytes: &[u8]) {
		if self.osc_fields == MAX_OSC_PARAMS {
			// After the last field's `;`, the rest of the string is dropped.
		} else if self.osc.len() + bytes.len() > MAX_OSC_BYTES {
			self.osc_overflow = true;
		} else {
			self.osc.extend_from_slice(bytes);
		}
	}

	fn osc_dispatch<H: Handler>(&mut self, handler: &mut H, bell: bool) {
		if self.osc_overflow {
			// Drop the buffer instead of keeping megabytes allocated.
			self.osc = Vec::new();
			return;
		}
		let mut fields: [&[u8]; MAX_OSC_PARAMS] = [&[]; MAX_OSC_PARAMS];
		let mut start = 0;
		for (field, &end) in fields.iter_mut().zip(&self.osc_ends[..self.osc_fields]) {
			*field = &self.osc[start..end];
			start = end;
		}
		let mut count = self.osc_fields;
		if count < MAX_OSC_PARAMS {
			fields[count] = &self.osc[start..];
			count += 1;
		}
		handler.osc_dispatch(&fields[..count], bell);
	}

	/// DCS payload: everything up to ESC, CAN, SUB or ST (`0x9c`), except
	/// DEL and other bytes >= 0x80, which are dropped.
	pub(super) fn dcs_passthrough<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let mut used = 0;
		loop {
			let run = simd::dcs_run(&bytes[used..]);
			if run > 0 {
				handler.put(&bytes[used..used + run]);
				used += run;
			}
			let Some(&byte) = bytes.get(used) else {
				return used;
			};
			used += 1;
			match byte {
				0x18 | 0x1a => {
					handler.unhook();
					handler.execute(byte);
					self.state = State::Ground;
					return used;
				}
				0x1b => {
					handler.unhook();
					self.enter_escape();
					return used;
				}
				0x9c => {
					handler.unhook();
					self.state = State::Ground;
					return used;
				}
				_ => {}
			}
		}
	}

	/// APC (delivered), and SOS, PM and malformed DCS strings (dropped),
	/// up to ESC, CAN or SUB.
	pub(super) fn ignored_string<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let apc = self.state == State::Apc;
		let run = simd::string_run(bytes);
		if apc && run > 0 {
			handler.apc_put(&bytes[..run]);
		}
		let Some(&byte) = bytes.get(run) else {
			return run;
		};
		if apc {
			handler.apc_end(byte == 0x1b);
		}
		if byte == 0x1b {
			self.enter_escape();
		} else {
			handler.execute(byte);
			self.state = State::Ground;
		}
		run + 1
	}
}
