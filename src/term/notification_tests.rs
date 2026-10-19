//! Desktop notifications: OSC 9, OSC 777 and kitty's OSC 99, from bytes in
//! to queued requests and replies out.
use super::notifications::{Event, Notification, Occasion, Request, Urgency};
use crate::{
	graphics::base64::encode,
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	term
		.responses
		.drain(..)
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

fn requests(term: &mut Term) -> Vec<Request> {
	std::mem::take(&mut term.notifications.requests)
}

/// The single notification shown since the last call.
fn next(term: &mut Term) -> Notification {
	match requests(term).as_slice() {
		[Request::Show(notification)] => (**notification).clone(),
		other => panic!("expected one notification, got {other:?}"),
	}
}

fn b64(text: &str) -> String {
	encode(text.as_bytes(), true)
}

#[test]
fn osc9_and_osc777_show_plain_notifications() {
	let mut t = terminal();
	send(&mut t, "\x1b]9;build; done\x07");
	let shown = next(&mut t);
	assert_eq!(
		(shown.title.as_str(), shown.body.as_str()),
		("build; done", "")
	);
	assert_eq!(shown.id, None);
	send(&mut t, "\x1b]777;notify;Title;Body; more\x1b\\");
	let shown = next(&mut t);
	assert_eq!(
		(shown.title.as_str(), shown.body.as_str()),
		("Title", "Body; more")
	);
	// Progress reports and other ConEmu commands are not notifications, and
	// neither is an empty message.
	send(
		&mut t,
		"\x1b]9;4;1;50\x07\x1b]9;1;100\x07\x1b]9;\x07\x1b]777;other;x\x07",
	);
	assert!(requests(&mut t).is_empty());
}

#[test]
fn osc99_assembles_chunks_by_id() {
	let mut t = terminal();
	send(&mut t, "\x1b]99;;Hello world\x1b\\");
	assert_eq!(next(&mut t).title, "Hello world");
	send(
		&mut t,
		"\x1b]99;i=1:d=0;Hello\x1b\\\x1b]99;i=2:d=0;Other\x1b\\\x1b]99;i=1:d=0:p=body;This is\x1b\\",
	);
	assert!(requests(&mut t).is_empty());
	send(&mut t, "\x1b]99;i=1:p=body:d=1; cool\x1b\\");
	let shown = next(&mut t);
	assert_eq!(shown.id.as_deref(), Some("1"));
	assert_eq!(
		(shown.title.as_str(), shown.body.as_str()),
		("Hello", "This is cool")
	);
	// Base64 split anywhere, or padded in each chunk.
	let title = b64("Grüße");
	let (first, rest) = title.split_at(3);
	send(
		&mut t,
		&format!(
			"\x1b]99;i=3:d=0:e=1;{first}\x1b\\\x1b]99;i=3:d=0:e=1;{rest}\x1b\\\
			 \x1b]99;i=3:d=0:e=1:p=body;{}\x1b\\\x1b]99;i=3:e=1:p=body;{}\x1b\\",
			b64("ab"),
			b64("cd")
		),
	);
	let shown = next(&mut t);
	assert_eq!(
		(shown.title.as_str(), shown.body.as_str()),
		("Grüße", "abcd")
	);
}

#[test]
fn osc99_reads_every_metadata_key() {
	let mut t = terminal();
	send(
		&mut t,
		&format!(
			"\x1b]99;i=n:d=0:u=2:a=report,-focus:c=1:o=unfocused:w=5000:f={}:t={}:t={}:n={}:n={}:s={}:x=ignored;T\x1b\\\
			 \x1b]99;i=n:p=buttons;Yes\u{2028}No\x1b\\",
			b64("app"),
			b64("im.received"),
			b64("email"),
			b64("error"),
			b64("foo"),
			b64("silent"),
		),
	);
	let shown = next(&mut t);
	assert_eq!(shown.urgency, Urgency::Critical);
	assert!(!shown.focus);
	assert_eq!(shown.occasion, Occasion::Unfocused);
	assert_eq!(shown.expire, Some(5000));
	assert_eq!(shown.app.as_deref(), Some("app"));
	assert_eq!(shown.types, ["im.received", "email"]);
	assert_eq!(shown.icon_names, ["error", "foo"]);
	assert_eq!(shown.sound.as_deref(), Some("silent"));
	assert_eq!(shown.buttons, ["Yes", "No"]);
	// Defaults.
	send(&mut t, "\x1b]99;;x\x1b\\");
	let shown = next(&mut t);
	assert_eq!(shown.urgency, Urgency::Normal);
	assert!(shown.focus);
	assert_eq!((shown.occasion, shown.expire), (Occasion::Always, None));
	// Unknown values of known keys drop the notification.
	send(&mut t, "\x1b]99;u=7;x\x1b\\\x1b]99;o=sometimes;x\x1b\\");
	assert!(requests(&mut t).is_empty());
}

#[test]
fn osc99_answers_the_support_query() {
	let mut t = terminal();
	send(&mut t, "\x1b]99;i=q$1:p=?;\x1b\\");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]99;i=q1:p=?;a=focus,report:c=1:o=always,unfocused,invisible:\
		  p=title,body,close,icon,?,alive,buttons:s=system,silent,error,warn,warning,info,question:u=0,1,2:w=1\x1b\\"
		]
	);
	assert!(requests(&mut t).is_empty());
}

#[test]
fn activation_and_close_are_reported_when_asked() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]99;i=x:a=report:c=1;hi\x1b\\\x1b]99;a=report;anon\x1b\\\x1b]99;i=q;quiet\x1b\\",
	);
	let [Request::Show(x), Request::Show(anon), Request::Show(quiet)] = &requests(&mut t)[..] else {
		panic!("expected three notifications");
	};
	let (x, anon, quiet) = (x.token, anon.token, quiet.token);
	t.notification_event(x, Event::Activated(None));
	t.notification_event(x, Event::Activated(Some(2)));
	t.notification_event(anon, Event::Activated(None));
	t.notification_event(quiet, Event::Activated(None));
	t.notification_event(quiet, Event::Closed);
	t.notification_event(x, Event::Closed);
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]99;i=x;\x1b\\",
			"\x1b]99;i=x;2\x1b\\",
			"\x1b]99;i=0;\x1b\\",
			"\x1b]99;i=x:p=close;\x1b\\",
		]
	);
	// Closed notifications report nothing more.
	t.notification_event(x, Event::Activated(None));
	assert!(replies(&mut t).is_empty());
	// Platforms that cannot tell when a notification closes say so.
	send(&mut t, "\x1b]99;i=u:c=1;hi\x1b\\");
	let token = next(&mut t).token;
	t.notification_event(token, Event::Untracked);
	assert_eq!(replies(&mut t), ["\x1b]99;i=u:p=close;untracked\x1b\\"]);
}

#[test]
fn ids_update_close_and_list_live_notifications() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]99;i=a;one\x1b\\\x1b]99;i=b;two\x1b\\\x1b]99;;three\x1b\\",
	);
	let first = requests(&mut t);
	let Request::Show(a) = &first[0] else {
		panic!()
	};
	send(&mut t, "\x1b]99;i=a;one again\x1b\\");
	let again = next(&mut t);
	assert_eq!(again.replaces, Some(a.token));
	send(&mut t, "\x1b]99;i=me:p=alive;\x07");
	assert_eq!(replies(&mut t), ["\x1b]99;i=me:p=alive;b,a\x07"]);
	send(
		&mut t,
		"\x1b]99;i=b:p=close;\x1b\\\x1b]99;p=close;\x1b\\\x1b]99;i=zz:p=close;\x1b\\",
	);
	let Request::Show(b) = &first[1] else {
		panic!()
	};
	assert_eq!(requests(&mut t), [Request::Close(b.token)]);
	// Without an id nothing is replaced.
	send(&mut t, "\x1b]99;;three\x1b\\");
	assert_eq!(next(&mut t).replaces, None);
}

#[test]
fn icons_are_sent_cached_and_bounded() {
	let mut t = terminal();
	let png = encode(b"\x89PNG fake", true);
	send(
		&mut t,
		&format!("\x1b]99;i=1:d=0:g=logo:p=icon:e=1;{png}\x1b\\\x1b]99;i=1;T\x1b\\"),
	);
	assert_eq!(next(&mut t).icon.as_deref(), Some(&b"\x89PNG fake"[..]));
	send(&mut t, "\x1b]99;g=logo;T\x1b\\");
	assert_eq!(next(&mut t).icon.as_deref(), Some(&b"\x89PNG fake"[..]));
	// Icon data must be base64.
	send(
		&mut t,
		"\x1b]99;i=2:d=0:p=icon;raw\x1b\\\x1b]99;i=2;T\x1b\\",
	);
	assert_eq!(next(&mut t).icon, None);
}

#[test]
fn untrusted_text_is_sanitized_and_bounded() {
	let mut t = terminal();
	// Control characters in plain text drop the chunk; titles are bounded.
	send(&mut t, &format!("\x1b]99;e=1;{}\x1b\\", b64("a\nb")));
	assert!(requests(&mut t).is_empty());
	let long = "x".repeat(3000);
	send(
		&mut t,
		&format!("\x1b]99;i=l:d=0;{long}\x1b\\\x1b]99;i=l;{long}\x1b\\"),
	);
	assert_eq!(next(&mut t).title.len(), super::notifications::MAX_TEXT);
	// Abandoned drafts are bounded too.
	for index in 0..100 {
		send(&mut t, &format!("\x1b]99;i={index}:d=0;x\x1b\\"));
	}
	assert!(t.notifications.drafts() <= super::notifications::MAX_DRAFTS);
	// Notifications without text are ignored.
	send(&mut t, "\x1b]99;i=e:p=body;\x1b\\");
	assert!(requests(&mut t).is_empty());
}
