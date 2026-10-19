//! Desktop notifications programs request (OSC 9, OSC 777, kitty's OSC 99):
//! notifications being assembled from chunks, requests queued for the UI
//! thread, and the notifications still shown, for reports and queries.
use std::sync::Arc;

use crate::graphics::base64::Decoder;

/// Most bytes of a title or body.
pub(crate) const MAX_TEXT: usize = 4096;
/// Most notifications being assembled at once; older ones are dropped.
pub(crate) const MAX_DRAFTS: usize = 8;
/// Most entries in a list: buttons, icon names, types.
pub(crate) const MAX_LIST: usize = 16;
/// Most bytes of a button, icon name, type, sound or application name.
pub(crate) const MAX_NAME: usize = 256;
/// Most bytes of one icon image.
pub(crate) const MAX_ICON: usize = 1 << 20;
/// Most bytes of icons cached by key; the least recently used go first.
const MAX_ICON_CACHE: usize = 8 << 20;
/// Most notifications remembered as shown.
const MAX_LIVE: usize = 64;
/// Requests waiting for the UI thread; older ones are dropped.
const MAX_QUEUED: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Urgency {
	Low,
	#[default]
	Normal,
	Critical,
}

/// When the notification should be shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Occasion {
	#[default]
	Always,
	/// Only when the terminal does not have keyboard focus.
	Unfocused,
	/// Only when the session is not visible either.
	Invisible,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Notification {
	/// Names this notification in [`Event`]s and [`Request::Close`].
	pub token: u64,
	/// The shown notification this one replaces.
	pub replaces: Option<u64>,
	/// The program's identifier, sanitized.
	pub id: Option<String>,
	pub title: String,
	pub body: String,
	pub urgency: Urgency,
	/// Activating it focuses the session.
	pub focus: bool,
	/// Activating it is reported to the program.
	pub report: bool,
	/// Closing it is reported to the program.
	pub report_close: bool,
	pub occasion: Occasion,
	/// Milliseconds until it closes: `Some(0)` never, `None` as the desktop
	/// decides.
	pub expire: Option<u32>,
	pub buttons: Vec<String>,
	pub icon_names: Vec<String>,
	/// PNG, JPEG or GIF data.
	pub icon: Option<Arc<[u8]>>,
	pub app: Option<String>,
	pub types: Vec<String>,
	pub sound: Option<String>,
}

impl Default for Notification {
	fn default() -> Self {
		Self {
			token: 0,
			replaces: None,
			id: None,
			title: String::new(),
			body: String::new(),
			urgency: Urgency::Normal,
			focus: true,
			report: false,
			report_close: false,
			occasion: Occasion::Always,
			expire: None,
			buttons: Vec::new(),
			icon_names: Vec::new(),
			icon: None,
			app: None,
			types: Vec::new(),
			sound: None,
		}
	}
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
	Show(Box<Notification>),
	Close(u64),
}

/// What happened to a shown notification, as the desktop reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
	/// Clicked, or one of its buttons (from 1) was. Not reported on macOS.
	#[cfg_attr(target_os = "macos", allow(dead_code))]
	Activated(Option<u32>),
	Closed,
	/// The desktop will not say when it closes (macOS).
	#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
	Untracked,
}

/// Bytes of a text or icon payload received in chunks, maybe as base64.
#[derive(Clone, Default)]
pub(crate) struct Field {
	bytes: Vec<u8>,
	decoder: Decoder,
	limit_hit: bool,
	invalid: bool,
}

impl Field {
	/// Adds a chunk. Base64 chunks may be split anywhere, or each padded.
	pub(crate) fn push(&mut self, chunk: &[u8], base64: bool, limit: usize) {
		if self.invalid || self.limit_hit {
			return;
		}
		if !base64 {
			self.bytes.extend_from_slice(chunk);
		} else {
			if self.decoder.padded() {
				self.decoder = Decoder::default();
			}
			if self.decoder.push(chunk, &mut self.bytes).is_err() {
				self.invalid = true;
			}
		}
		if self.bytes.len() > limit {
			self.bytes.truncate(limit);
			self.limit_hit = true;
		}
	}

	/// Marks the payload unusable.
	pub(crate) fn reject(&mut self) {
		self.invalid = true;
	}

	/// The text received, cut to whole characters if it was too long; `None`
	/// if it is not UTF-8 or holds control characters.
	fn text(mut self) -> Option<String> {
		if self.invalid || self.decoder.finish(&mut self.bytes).is_err() {
			return None;
		}
		let text = match String::from_utf8(self.bytes) {
			Ok(text) => text,
			Err(error) if self.limit_hit && error.utf8_error().error_len().is_none() => {
				let valid = error.utf8_error().valid_up_to();
				let mut bytes = error.into_bytes();
				bytes.truncate(valid);
				String::from_utf8(bytes).ok()?
			}
			Err(_) => return None,
		};
		(!text.chars().any(char::is_control)).then_some(text)
	}

	fn data(mut self) -> Option<Vec<u8>> {
		let valid = !self.invalid && !self.limit_hit && self.decoder.finish(&mut self.bytes).is_ok();
		(valid && !self.bytes.is_empty()).then_some(self.bytes)
	}
}

/// A notification being assembled from OSC 99 chunks.
#[derive(Clone, Default)]
pub(crate) struct Draft {
	pub notification: Notification,
	pub title: Field,
	pub body: Field,
	pub buttons: Field,
	/// Icon data, which is always base64.
	pub icon: Field,
	pub icon_key: Option<String>,
}

#[derive(Clone, Debug)]
struct Live {
	token: u64,
	id: Option<String>,
	report: bool,
	report_close: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Notifications {
	/// Requests for the UI thread, oldest first.
	pub requests: Vec<Request>,
	drafts: Vec<(Option<String>, Draft)>,
	live: Vec<Live>,
	/// Icon data by key, least recently used first.
	icons: Vec<(String, Arc<[u8]>)>,
	next_token: u64,
}

impl Notifications {
	/// Notifications being assembled.
	#[cfg(test)]
	pub(crate) fn drafts(&self) -> usize {
		self.drafts.len()
	}

	/// The draft for `id`, started if there is none.
	pub(crate) fn draft(&mut self, id: Option<String>) -> &mut Draft {
		let index = match self.drafts.iter().position(|(key, _)| *key == id) {
			Some(index) => index,
			None => {
				if self.drafts.len() >= MAX_DRAFTS {
					self.drafts.remove(0);
				}
				let notification = Notification {
					id: id.clone(),
					..Notification::default()
				};
				let draft = Draft {
					notification,
					..Draft::default()
				};
				self.drafts.push((id, draft));
				self.drafts.len() - 1
			}
		};
		&mut self.drafts[index].1
	}

	/// Shows the draft for `id`, if it has any text.
	pub(crate) fn finish(&mut self, id: &Option<String>) {
		let Some(index) = self.drafts.iter().position(|(key, _)| key == id) else {
			return;
		};
		let (_, draft) = self.drafts.remove(index);
		let mut notification = draft.notification;
		let (Some(title), Some(body), Some(buttons)) =
			(draft.title.text(), draft.body.text(), draft.buttons.text())
		else {
			return;
		};
		(notification.title, notification.body) = if title.is_empty() {
			(body, String::new())
		} else {
			(title, body)
		};
		if notification.title.is_empty() {
			return;
		}
		notification.buttons = buttons
			.split('\u{2028}')
			.filter(|button| !button.is_empty())
			.take(MAX_LIST)
			.map(|button| truncate(button, MAX_NAME).to_owned())
			.collect();
		notification.icon = match (draft.icon.data(), draft.icon_key) {
			(Some(data), key) => {
				let data: Arc<[u8]> = data.into();
				if let Some(key) = key {
					self.cache_icon(key, data.clone());
				}
				Some(data)
			}
			(None, Some(key)) => self.cached_icon(&key),
			(None, None) => None,
		};
		self.show(notification);
	}

	/// Queues `notification`, replacing a shown one with the same id.
	pub(crate) fn show(&mut self, mut notification: Notification) {
		self.next_token += 1;
		notification.token = self.next_token;
		if notification.id.is_some()
			&& let Some(index) = self.live.iter().position(|live| live.id == notification.id)
		{
			notification.replaces = Some(self.live.remove(index).token);
		}
		if self.live.len() >= MAX_LIVE {
			self.live.remove(0);
		}
		self.live.push(Live {
			token: notification.token,
			id: notification.id.clone(),
			report: notification.report,
			report_close: notification.report_close,
		});
		self.queue(Request::Show(Box::new(notification)));
	}

	/// Asks the UI thread to close the shown notification called `id`.
	pub(crate) fn close(&mut self, id: &str) {
		if let Some(live) = self.live.iter().find(|live| live.id.as_deref() == Some(id)) {
			let token = live.token;
			self.queue(Request::Close(token));
		}
	}

	/// The ids of shown notifications, oldest first.
	pub(crate) fn alive(&self) -> Vec<&str> {
		self
			.live
			.iter()
			.filter_map(|live| live.id.as_deref())
			.collect()
	}

	/// The reply `event` calls for, if the program asked for one.
	pub(crate) fn event(&mut self, token: u64, event: Event) -> Option<String> {
		let index = self.live.iter().position(|live| live.token == token)?;
		let live = &self.live[index];
		let id = live.id.as_deref().unwrap_or("0");
		match event {
			Event::Activated(button) => live.report.then(|| {
				let button = button.map(|button| button.to_string()).unwrap_or_default();
				format!("i={id};{button}")
			}),
			Event::Untracked => live
				.report_close
				.then(|| format!("i={id}:p=close;untracked")),
			Event::Closed => {
				let reply = live.report_close.then(|| format!("i={id}:p=close;"));
				self.live.remove(index);
				reply
			}
		}
	}

	fn queue(&mut self, request: Request) {
		if self.requests.len() >= MAX_QUEUED {
			self.requests.remove(0);
		}
		self.requests.push(request);
	}

	fn cache_icon(&mut self, key: String, data: Arc<[u8]>) {
		self.icons.retain(|(cached, _)| *cached != key);
		self.icons.push((key, data));
		let mut total: usize = self.icons.iter().map(|(_, data)| data.len()).sum();
		while total > MAX_ICON_CACHE {
			total -= self.icons.remove(0).1.len();
		}
	}

	fn cached_icon(&mut self, key: &str) -> Option<Arc<[u8]>> {
		let index = self.icons.iter().position(|(cached, _)| cached == key)?;
		let entry = self.icons.remove(index);
		let data = entry.1.clone();
		self.icons.push(entry);
		Some(data)
	}
}

/// `text` cut to at most `limit` bytes of whole characters.
pub(crate) fn truncate(text: &str, limit: usize) -> &str {
	&text[..text.floor_char_boundary(limit)]
}

impl super::Term {
	/// Records what happened to a shown notification and reports it to the
	/// program if it asked.
	pub(crate) fn notification_event(&mut self, token: u64, event: Event) {
		if let Some(reply) = self.notifications.event(token, event) {
			self
				.responses
				.push(format!("\x1b]99;{reply}\x1b\\").into_bytes());
		}
	}
}
