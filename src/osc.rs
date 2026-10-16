//! OSC 133 semantic shell markers and OSC 7 directory URIs.
use std::{collections::VecDeque, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
	#[default]
	Unknown,
	Prompt,
	Command,
	Output,
	Finished,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
	pub phase: Phase,
	pub row: usize,
	pub col: usize,
	pub exit_code: Option<i32>,
}
#[derive(Clone, Default)]
pub(crate) struct ShellIntegration {
	pub phase: Phase,
	pub last_exit_code: Option<i32>,
	pub markers: VecDeque<Marker>,
}
impl ShellIntegration {
	pub fn record(&mut self, params: &[&[u8]], row: usize, col: usize) {
		let phase = match params.get(1).copied() {
			Some(b"A") => Phase::Prompt,
			Some(b"B") => Phase::Command,
			Some(b"C") => Phase::Output,
			Some(b"D") => Phase::Finished,
			_ => return,
		};
		let exit_code = if phase == Phase::Finished {
			params
				.get(2)
				.and_then(|p| std::str::from_utf8(p).ok())
				.and_then(|s| s.parse().ok())
		} else {
			None
		};
		if phase == Phase::Finished {
			self.last_exit_code = exit_code;
		}
		if phase == Phase::Output {
			self.last_exit_code = None;
		}
		self.phase = phase;
		self.markers.push_back(Marker {
			phase,
			row,
			col,
			exit_code,
		});
		if self.markers.len() > 256 {
			self.markers.pop_front();
		}
	}
}

pub(crate) fn decode_uri_path(path: &str) -> Option<PathBuf> {
	let mut decoded = Vec::with_capacity(path.len());
	let mut bytes = path.bytes();
	while let Some(byte) = bytes.next() {
		let byte = if byte == b'%' {
			let high = (bytes.next()? as char).to_digit(16)?;
			let low = (bytes.next()? as char).to_digit(16)?;
			(high * 16 + low) as u8
		} else {
			byte
		};
		if byte == 0 || byte < 32 {
			return None;
		}
		decoded.push(byte);
	}
	let path = PathBuf::from(String::from_utf8(decoded).ok()?);
	path.is_absolute().then_some(path)
}
