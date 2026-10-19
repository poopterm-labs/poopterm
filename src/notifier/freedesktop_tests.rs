//! The freedesktop.org backend against a fake notification server on a
//! private bus: the Notify arguments, replacement, closing, and the
//! activation and close signals coming back.
use std::{sync::mpsc, time::Duration};

use super::freedesktop::Freedesktop;
use crate::{
	dbus::{
		Connection,
		message::{Kind, Message},
		test_bus::TestBus,
		wire::Value,
	},
	term::notifications::{Event, Notification, Urgency},
};

const SERVICE: &str = "org.freedesktop.Notifications";

fn reply(to: &Message, body: Vec<Value>) -> Message {
	let mut reply = Message::method_call("", "/", "", "");
	(reply.kind, reply.path, reply.interface, reply.member) = (Kind::Return, None, None, None);
	reply.destination = to.sender.clone();
	reply.reply_serial = Some(to.serial);
	reply.body = body;
	reply
}

fn signal(member: &str, body: Vec<Value>) -> Message {
	let mut signal = Message::method_call("", "/org/freedesktop/Notifications", SERVICE, member);
	(signal.kind, signal.destination) = (Kind::Signal, None);
	signal.body = body;
	signal
}

fn hint<'a>(hints: &'a Value, name: &str) -> Option<&'a Value> {
	let Value::Array(_, entries) = hints else {
		panic!("hints are not an array")
	};
	entries.iter().find_map(|entry| match entry {
		Value::DictEntry(key, value) if key.as_str() == Some(name) => match &**value {
			Value::Variant(value) => Some(&**value),
			_ => None,
		},
		_ => None,
	})
}

#[test]
fn notifications_reach_the_server_and_its_signals_come_back() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut server = Connection::connect(&bus.address, None).unwrap();
	let mut request = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"RequestName",
	);
	request.body = vec![Value::Str(SERVICE.into()), Value::U32(0)];
	server.call(request).unwrap();
	let (mut server_tx, mut server_rx) = server.split();
	server_rx.set_timeout(Duration::from_secs(5));

	let (events_tx, events) = mpsc::channel();
	let notifier = Freedesktop::start(
		Some(bus.address.clone()),
		Box::new(move |key, event| events_tx.send((key, event)).unwrap()),
	);
	let mut call = |server_tx: &mut crate::dbus::Sender, member: &str, answer: Vec<Value>| loop {
		let message = server_rx.receive().unwrap();
		if message.member.as_deref() == Some(member) {
			server_tx.send(reply(&message, answer)).unwrap();
			return message;
		}
		if message.member.as_deref() == Some("GetCapabilities") {
			let capabilities = ["actions", "body", "body-markup"].map(|name| Value::Str(name.into()));
			server_tx
				.send(reply(
					&message,
					vec![Value::Array("s".into(), capabilities.to_vec())],
				))
				.unwrap();
		}
	};

	notifier.show(
		1,
		None,
		&Notification {
			title: "Build <done>".into(),
			body: "a & b <i>".into(),
			urgency: Urgency::Critical,
			buttons: vec!["Yes".into(), "No".into()],
			icon_names: vec!["error".into()],
			types: vec!["im.received".into()],
			sound: Some("silent".into()),
			expire: Some(5000),
			..Notification::default()
		},
	);
	let notify = call(&mut server_tx, "Notify", vec![Value::U32(7)]);
	let [app, replaces, icon, summary, body, actions, hints, expire] = &notify.body[..] else {
		panic!("Notify takes eight arguments: {:?}", notify.body);
	};
	assert_eq!(app.as_str(), Some("poopterm"));
	assert_eq!(replaces, &Value::U32(0));
	assert_eq!(icon.as_str(), Some("dialog-error"));
	// The server reads markup in the body only.
	assert_eq!(summary.as_str(), Some("Build <done>"));
	assert_eq!(body.as_str(), Some("a &amp; b &lt;i&gt;"));
	let strings = |items: &[&str]| {
		Value::Array(
			"s".into(),
			items
				.iter()
				.map(|item| Value::Str((*item).into()))
				.collect(),
		)
	};
	assert_eq!(actions, &strings(&["default", "", "1", "Yes", "2", "No"]));
	assert_eq!(hint(hints, "urgency"), Some(&Value::Byte(2)));
	assert_eq!(
		hint(hints, "category"),
		Some(&Value::Str("im.received".into()))
	);
	assert_eq!(hint(hints, "suppress-sound"), Some(&Value::Bool(true)));
	assert_eq!(
		hint(hints, "desktop-entry"),
		Some(&Value::Str("poopterm".into()))
	);
	assert_eq!(expire, &Value::I32(5000));

	// An update replaces the shown notification.
	notifier.show(
		2,
		Some(1),
		&Notification {
			title: "again".into(),
			..Notification::default()
		},
	);
	let notify = call(&mut server_tx, "Notify", vec![Value::U32(7)]);
	assert_eq!(notify.body[1], Value::U32(7));

	server_tx
		.send(signal(
			"ActionInvoked",
			vec![Value::U32(7), Value::Str("2".into())],
		))
		.unwrap();
	server_tx
		.send(signal(
			"ActionInvoked",
			vec![Value::U32(7), Value::Str("default".into())],
		))
		.unwrap();
	server_tx
		.send(signal(
			"NotificationClosed",
			vec![Value::U32(7), Value::U32(2)],
		))
		.unwrap();
	// Signals about notifications of other programs are not ours.
	server_tx
		.send(signal(
			"NotificationClosed",
			vec![Value::U32(99), Value::U32(2)],
		))
		.unwrap();
	let received: Vec<_> = (0..3)
		.map(|_| events.recv_timeout(Duration::from_secs(5)).unwrap())
		.collect();
	assert_eq!(
		received,
		[
			(2, Event::Activated(Some(2))),
			(2, Event::Activated(None)),
			(2, Event::Closed)
		]
	);

	notifier.show(
		3,
		None,
		&Notification {
			title: "third".into(),
			..Notification::default()
		},
	);
	call(&mut server_tx, "Notify", vec![Value::U32(8)]);
	notifier.close(3);
	let close = call(&mut server_tx, "CloseNotification", vec![]);
	assert_eq!(close.body, [Value::U32(8)]);
}
