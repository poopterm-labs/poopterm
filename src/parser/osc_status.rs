//! Program status OSCs: OSC 7501 records, checked against every limit of
//! the specification before anything is stored, and OSC 9;4 progress.
use crate::{
	graphics::base64,
	term::{
		Term,
		status::{Kind, Progress, ProgressState, Record, State},
	},
};

/// Longest report body: the whole sequence may take 4096 bytes.
const MAX_BODY: usize = 4096 - "\x1b]7501;\x1b\\".len();
const MAX_KEY: usize = 16;
/// Encoded and decoded limits of `msg` and `title`.
const MSG: (usize, usize) = (2732, 2048);
const TITLE: (usize, usize) = (256, 192);
const MAX_APP: usize = 32;
const MAX_ID: usize = 128;
const MAX_SEGMENT: usize = 32;
const MAX_LEVELS: usize = 8;

impl Term {
	/// OSC 7501: `params` follow the command number.
	pub(super) fn osc_program_status(&mut self, params: &[&[u8]], bell: bool) {
		let body = params.join(&b';');
		if body.trim_ascii() == b"?" {
			let reply = "7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";
			return self.osc_reply(reply.to_owned(), bell);
		}
		match parse_report(&body) {
			Some(Report::Set(record)) => self.status.set(record),
			Some(Report::Clear(id)) => self.status.clear(&id),
			None => {}
		}
	}

	/// OSC 9;4: `params` follow the 4.
	pub(super) fn osc_progress(&mut self, params: &[&[u8]]) {
		let percent = params
			.get(1)
			.filter(|digits| {
				!digits.is_empty() && digits.len() <= 5 && digits.iter().all(u8::is_ascii_digit)
			})
			.and_then(|digits| std::str::from_utf8(digits).ok()?.parse::<u32>().ok())
			.map(|percent| percent.min(100) as u8);
		let state = match params.first().copied().unwrap_or_default() {
			b"" | b"0" => return self.status.set_progress(None),
			b"1" => ProgressState::Normal,
			b"2" => ProgressState::Error,
			b"3" => ProgressState::Indeterminate,
			b"4" => ProgressState::Paused,
			_ => return,
		};
		let percent = if state == ProgressState::Indeterminate {
			None
		} else {
			percent
		};
		self.status.set_progress(Some(Progress { state, percent }));
	}
}

enum Report {
	Set(Record),
	Clear(String),
}

/// The pairs of a report, or `None` if it breaks a limit or is invalid.
fn parse_report(body: &[u8]) -> Option<Report> {
	if body.len() > MAX_BODY {
		return None;
	}
	let mut keys: [Option<&[u8]>; 7] = [None; 7];
	const NAMES: [&[u8]; 7] = [
		b"state",
		b"id",
		b"kind",
		b"progress",
		b"app",
		b"title",
		b"msg",
	];
	for pair in body.split(|&byte| byte == b':') {
		let Some(at) = pair.iter().position(|&byte| byte == b'=') else {
			continue;
		};
		let (key, value) = (pair[..at].trim_ascii(), pair[at + 1..].trim_ascii());
		if key.is_empty() || !key.iter().all(u8::is_ascii_lowercase) {
			continue;
		}
		if key.len() > MAX_KEY {
			return None;
		}
		let valid = |byte: &u8| byte.is_ascii_alphanumeric() || b"_.,+/=-".contains(byte);
		if !value.iter().all(valid) {
			continue;
		}
		if let Some(slot) = NAMES.iter().position(|name| *name == key) {
			keys[slot] = Some(value);
		}
	}
	let [state, id, kind, progress, app, title, msg] = keys;
	let id = match id {
		Some(id) => parse_id(id)?,
		None => String::new(),
	};
	let state = match state? {
		b"idle" => State::Idle,
		b"working" => State::Working,
		b"done" => State::Done,
		b"blocked" => State::Blocked,
		b"error" => State::Error,
		b"clear" => return Some(Report::Clear(id)),
		_ => return None,
	};
	let app = match app {
		Some(app) if app.len() > MAX_APP => return None,
		Some(app) if !app.is_empty() && app.iter().all(|&byte| segment_byte(byte)) => {
			Some(String::from_utf8(app.to_vec()).ok()?)
		}
		_ => None,
	};
	let (title, msg) = match (title, msg) {
		(Some(title), _) if text(title, TITLE).is_none() => return None,
		(_, Some(msg)) if text(msg, MSG).is_none() => return None,
		_ => (
			title.and_then(|title| text(title, TITLE)),
			msg.and_then(|msg| text(msg, MSG)),
		),
	};
	let kind = match (state, kind) {
		(State::Blocked, Some(b"permission")) => Some(Kind::Permission),
		(State::Blocked, Some(b"question")) => Some(Kind::Question),
		(State::Blocked, Some(b"auth")) => Some(Kind::Auth),
		_ => None,
	};
	let progress = progress
		.filter(|_| matches!(state, State::Working | State::Blocked))
		.filter(|digits| {
			!digits.is_empty() && digits.len() <= 3 && digits.iter().all(u8::is_ascii_digit)
		})
		.and_then(|digits| std::str::from_utf8(digits).ok()?.parse::<u8>().ok())
		.filter(|&percent| percent <= 100);
	Some(Report::Set(Record {
		id,
		state,
		kind,
		progress,
		app,
		title,
		msg,
	}))
}

fn segment_byte(byte: u8) -> bool {
	byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte)
}

/// A record id: up to eight `/`-separated segments.
fn parse_id(id: &[u8]) -> Option<String> {
	if id.len() > MAX_ID {
		return None;
	}
	let mut levels = 0;
	for segment in id.split(|&byte| byte == b'/') {
		levels += 1;
		if levels > MAX_LEVELS
			|| segment.is_empty()
			|| segment.len() > MAX_SEGMENT
			|| !segment.iter().all(|&byte| segment_byte(byte))
		{
			return None;
		}
	}
	String::from_utf8(id.to_vec()).ok()
}

/// Base64 UTF-8 text within `(encoded, decoded)` byte limits, without
/// control characters.
fn text(value: &[u8], (encoded, decoded): (usize, usize)) -> Option<String> {
	if value.len() > encoded {
		return None;
	}
	let bytes = base64::decode(value)?;
	if bytes.len() > decoded {
		return None;
	}
	let text = String::from_utf8(bytes).ok()?;
	(!text.chars().any(char::is_control)).then_some(text)
}
