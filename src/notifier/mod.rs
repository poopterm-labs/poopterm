//! Delivers the notifications programs request to the desktop: the
//! freedesktop.org service on Linux and the BSDs, Notification Center on
//! macOS. Decides whether to show them at all: the program's occasion and
//! a rate limit per session.
#[cfg(not(target_os = "macos"))]
mod freedesktop;
#[cfg(all(test, not(target_os = "macos")))]
mod freedesktop_tests;
#[cfg(target_os = "macos")]
mod macos;
mod rate;
mod routes;

pub(crate) use rate::RateLimit;
pub(crate) use routes::Routes;

use crate::term::notifications::{Event, Notification, Occasion};

/// Called from a background thread when the desktop reports on the
/// notification shown with a key.
pub(crate) type Report = Box<dyn Fn(u64, Event) + Send>;

/// The platform's notification service.
pub(crate) struct Notifier {
	#[cfg(not(target_os = "macos"))]
	backend: freedesktop::Freedesktop,
	#[cfg(target_os = "macos")]
	backend: macos::Macos,
}

impl Notifier {
	pub(crate) fn new(report: Report) -> Self {
		#[cfg(not(target_os = "macos"))]
		let backend = freedesktop::Freedesktop::start(None, report);
		#[cfg(target_os = "macos")]
		let backend = macos::Macos::new(report);
		Self { backend }
	}

	/// Shows `notification` as `key`, replacing the one shown as `replaces`.
	pub(crate) fn show(&self, key: u64, replaces: Option<u64>, notification: &Notification) {
		self.backend.show(key, replaces, notification);
	}

	pub(crate) fn close(&self, key: u64) {
		self.backend.close(key);
	}
}

/// Whether a notification for `occasion` is shown, given whether the
/// window has keyboard focus and the session can be seen.
pub(crate) fn wanted(occasion: Occasion, focused: bool, visible: bool) -> bool {
	match occasion {
		Occasion::Always => true,
		Occasion::Unfocused => !focused,
		Occasion::Invisible => !focused && !visible,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn occasions_follow_focus_and_visibility() {
		let cases = [
			(Occasion::Always, true, true, true),
			(Occasion::Unfocused, true, true, false),
			(Occasion::Unfocused, false, true, true),
			(Occasion::Invisible, false, true, false),
			(Occasion::Invisible, false, false, true),
			(Occasion::Invisible, true, false, false),
		];
		for (occasion, focused, visible, shown) in cases {
			assert_eq!(
				wanted(occasion, focused, visible),
				shown,
				"{occasion:?} {focused} {visible}"
			);
		}
	}
}
