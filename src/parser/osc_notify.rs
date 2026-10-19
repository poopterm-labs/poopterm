//! Notification OSCs: iTerm2's OSC 9, OSC 777 `notify` and kitty's OSC 99,
//! whose metadata, chunks and queries this module reads.
use crate::{
	graphics::base64,
	term::{
		Term,
		notifications::{
			Field, MAX_ICON, MAX_LIST, MAX_NAME, MAX_TEXT, Notification, Occasion, Urgency, truncate,
		},
	},
};

/// What an OSC 99 query reply says the terminal supports.
const SUPPORT: &str = "a=focus,report:c=1:o=always,unfocused,invisible:\
	p=title,body,close,icon,?,alive,buttons:\
	s=system,silent,error,warn,warning,info,question:u=0,1,2:w=1";

impl Term {
	/// OSC 9: a notification, unless the first parameter is a number, which
	/// makes it a ConEmu command; 4 is progress.
	pub(super) fn osc_notify(&mut self, params: &[&[u8]]) {
		let Some(first) = params.first() else {
			return;
		};
		if *first == b"4" {
			return self.osc_progress(&params[1..]);
		}
		if !first.is_empty() && first.iter().all(u8::is_ascii_digit) {
			return self.osc_conemu(params);
		}
		self.notify_text(b"", &params.join(&b';'));
	}

	/// OSC 777 `notify;title;body`.
	pub(super) fn osc_notify_777(&mut self, params: &[&[u8]]) {
		if let [b"notify", title, body @ ..] = params {
			self.notify_text(title, &body.join(&b';'));
		}
	}

	fn notify_text(&mut self, title: &[u8], body: &[u8]) {
		let clean = |text: &[u8]| {
			let text: String = String::from_utf8_lossy(text)
				.chars()
				.filter(|c| !c.is_control())
				.collect();
			truncate(&text, MAX_TEXT).to_owned()
		};
		let (title, body) = (clean(title), clean(body));
		let (title, body) = if title.is_empty() {
			(body, String::new())
		} else {
			(title, body)
		};
		if !title.is_empty() {
			self.notifications.show(Notification {
				title,
				body,
				..Notification::default()
			});
		}
	}

	/// kitty's OSC 99: `metadata` is `key=value` pairs separated by `:`.
	pub(super) fn osc_kitty_notify(&mut self, metadata: &[u8], payload: &[&[u8]], bell: bool) {
		let Some(meta) = Meta::parse(metadata) else {
			return;
		};
		let payload = payload.join(&b';');
		let id = meta.id.clone();
		let reply_id = id.as_deref().unwrap_or("0");
		let reply = match meta.payload {
			Payload::Query => format!("i={reply_id}:p=?;{SUPPORT}"),
			Payload::Alive => format!(
				"i={reply_id}:p=alive;{}",
				self.notifications.alive().join(",")
			),
			Payload::Close => {
				if let Some(id) = &id {
					self.notifications.close(id);
				}
				return;
			}
			Payload::Unknown => return,
			Payload::Title | Payload::Body | Payload::Buttons | Payload::Icon => {
				let draft = self.notifications.draft(id.clone());
				meta.apply(&mut draft.notification);
				if meta.icon_key.is_some() {
					draft.icon_key = meta.icon_key.clone();
				}
				let (field, limit): (&mut Field, _) = match meta.payload {
					Payload::Title => (&mut draft.title, MAX_TEXT),
					Payload::Body => (&mut draft.body, MAX_TEXT),
					Payload::Buttons => (&mut draft.buttons, MAX_LIST * (MAX_NAME + 3)),
					_ => (&mut draft.icon, MAX_ICON),
				};
				// Icon data is binary, so it must be encoded.
				if meta.payload == Payload::Icon && !meta.base64 {
					field.reject();
				} else {
					field.push(&payload, meta.base64, limit);
				}
				if meta.done {
					self.notifications.finish(&id);
				}
				return;
			}
		};
		self.osc_reply(format!("99;{reply}"), bell);
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Payload {
	Title,
	Body,
	Close,
	Icon,
	Query,
	Alive,
	Buttons,
	Unknown,
}

/// The keys of one OSC 99 chunk; `None` fields were not given.
struct Meta {
	id: Option<String>,
	done: bool,
	base64: bool,
	payload: Payload,
	report: Option<bool>,
	focus: Option<bool>,
	report_close: Option<bool>,
	app: Option<String>,
	icon_key: Option<String>,
	icon_names: Vec<String>,
	occasion: Option<Occasion>,
	sound: Option<String>,
	types: Vec<String>,
	urgency: Option<Urgency>,
	expire: Option<Option<u32>>,
}

impl Meta {
	/// `None` when a known key has a value this terminal does not know.
	fn parse(metadata: &[u8]) -> Option<Self> {
		let mut meta = Self {
			id: None,
			done: true,
			base64: false,
			payload: Payload::Title,
			report: None,
			focus: None,
			report_close: None,
			app: None,
			icon_key: None,
			icon_names: Vec::new(),
			occasion: None,
			sound: None,
			types: Vec::new(),
			urgency: None,
			expire: None,
		};
		for pair in metadata.split(|&byte| byte == b':') {
			let [key, b'=', value @ ..] = pair else {
				continue;
			};
			match key {
				b'a' => {
					for action in value.split(|&byte| byte == b',') {
						let (on, action) = match action.strip_prefix(b"-") {
							Some(action) => (false, action),
							None => (true, action),
						};
						match action {
							b"report" => meta.report = Some(on),
							b"focus" => meta.focus = Some(on),
							_ => {}
						}
					}
				}
				b'c' => meta.report_close = Some(flag(value)?),
				b'd' => meta.done = flag(value)?,
				b'e' => meta.base64 = flag(value)?,
				b'f' => meta.app = name(value).or(meta.app),
				b'g' => meta.icon_key = Some(identifier(value)).filter(|key| !key.is_empty()),
				b'i' => meta.id = Some(identifier(value)).filter(|id| !id.is_empty()),
				b'n' => meta.icon_names.extend(name(value)),
				b'o' => {
					meta.occasion = Some(match value {
						b"always" => Occasion::Always,
						b"unfocused" => Occasion::Unfocused,
						b"invisible" => Occasion::Invisible,
						_ => return None,
					});
				}
				b'p' => {
					meta.payload = match value {
						b"title" => Payload::Title,
						b"body" => Payload::Body,
						b"close" => Payload::Close,
						b"icon" => Payload::Icon,
						b"?" => Payload::Query,
						b"alive" => Payload::Alive,
						b"buttons" => Payload::Buttons,
						_ => Payload::Unknown,
					};
				}
				b's' => meta.sound = name(value).or(meta.sound),
				b't' => meta.types.extend(name(value)),
				b'u' => {
					meta.urgency = Some(match value {
						b"0" => Urgency::Low,
						b"1" => Urgency::Normal,
						b"2" => Urgency::Critical,
						_ => return None,
					});
				}
				b'w' => {
					let millis: i64 = std::str::from_utf8(value).ok()?.parse().ok()?;
					meta.expire = Some(match millis {
						-1 => None,
						0.. => Some(u32::try_from(millis).unwrap_or(u32::MAX)),
						_ => return None,
					});
				}
				_ => {}
			}
		}
		Some(meta)
	}

	/// Sets the keys this chunk gave on `notification`.
	fn apply(&self, notification: &mut Notification) {
		let set = |target: &mut bool, value: Option<bool>| *target = value.unwrap_or(*target);
		set(&mut notification.report, self.report);
		set(&mut notification.focus, self.focus);
		set(&mut notification.report_close, self.report_close);
		notification.urgency = self.urgency.unwrap_or(notification.urgency);
		notification.occasion = self.occasion.unwrap_or(notification.occasion);
		notification.expire = self.expire.unwrap_or(notification.expire);
		for (target, value) in [
			(&mut notification.app, &self.app),
			(&mut notification.sound, &self.sound),
		] {
			if value.is_some() {
				target.clone_from(value);
			}
		}
		for (target, values) in [
			(&mut notification.icon_names, &self.icon_names),
			(&mut notification.types, &self.types),
		] {
			let room = MAX_LIST.saturating_sub(target.len());
			target.extend(values.iter().take(room).cloned());
		}
	}
}

fn flag(value: &[u8]) -> Option<bool> {
	match value {
		b"0" => Some(false),
		b"1" => Some(true),
		_ => None,
	}
}

/// A base64 value holding a short name without control characters.
fn name(value: &[u8]) -> Option<String> {
	if value.len() > MAX_NAME.div_ceil(3) * 4 {
		return None;
	}
	let text = String::from_utf8(base64::decode(value)?).ok()?;
	(!text.is_empty() && !text.chars().any(char::is_control)).then_some(text)
}

/// An identifier a program sent, reduced to the characters the protocols
/// allow (`[a-zA-Z0-9_+.-]`) so it is safe to send back.
pub(super) fn identifier(value: &[u8]) -> String {
	value
		.iter()
		.filter(|&&byte| byte.is_ascii_alphanumeric() || b"-_+.".contains(&byte))
		.take(64)
		.map(|&byte| byte as char)
		.collect()
}
