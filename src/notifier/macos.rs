//! Notification Center through `osascript`. The text travels as arguments,
//! never as script source, so it cannot inject AppleScript. macOS does not
//! report activation or closing to scripts.
use std::{process::Command, thread};

use super::Report;
use crate::term::notifications::{Event, Notification};

const SCRIPT: &str = "on run argv
	if item 3 of argv is \"\" then
		display notification (item 2 of argv) with title (item 1 of argv)
	else
		display notification (item 2 of argv) with title (item 1 of argv) sound name (item 3 of argv)
	end if
end run";

pub(crate) struct Macos {
	report: Report,
}

impl Macos {
	pub(crate) fn new(report: Report) -> Self {
		Self { report }
	}

	pub(crate) fn show(&self, key: u64, _replaces: Option<u64>, notification: &Notification) {
		let sound = match notification.sound.as_deref() {
			Some("silent") => "",
			Some("error") => "Basso",
			Some("warn" | "warning") => "Funk",
			Some("question") => "Purr",
			Some("info") => "Glass",
			_ => "default",
		};
		let child = Command::new("osascript")
			.args(["-e", SCRIPT, "--"])
			.args([
				notification.title.as_str(),
				notification.body.as_str(),
				sound,
			])
			.spawn();
		if let Ok(mut child) = child {
			let _ = thread::Builder::new()
				.name("osascript".into())
				.spawn(move || child.wait());
		}
		(self.report)(key, Event::Untracked);
	}

	pub(crate) fn close(&self, _key: u64) {}
}
