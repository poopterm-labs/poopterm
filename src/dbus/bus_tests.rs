//! The client against a real `dbus-daemon`: authentication, calls, replies
//! and signals between two connections.
use super::{
	Connection,
	message::{Kind, Message},
	test_bus::TestBus,
	wire::Value,
};

#[test]
fn connections_call_each_other_and_receive_signals() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut server = Connection::connect(&bus.address, None).unwrap();
	let mut client = Connection::connect(&bus.address, None).unwrap();
	assert!(server.unique_name().starts_with(':'));
	assert_ne!(server.unique_name(), client.unique_name());
	let mut request = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"RequestName",
	);
	request.body = vec![Value::Str("org.poopterm.Test".into()), Value::U32(0)];
	assert_eq!(server.call(request).unwrap().body, [Value::U32(1)]);
	client
		.add_match("type='signal',interface='org.poopterm.Test'")
		.unwrap();

	let (mut server_tx, mut server_rx) = server.split();
	let (mut client_tx, mut client_rx) = client.split();
	let mut ping = Message::method_call("org.poopterm.Test", "/test", "org.poopterm.Test", "Ping");
	let entry = Value::DictEntry(
		Box::new(Value::Str("urgency".into())),
		Box::new(Value::Variant(Box::new(Value::Byte(2)))),
	);
	ping.body = vec![
		Value::Str("hi ünïcode".into()),
		Value::Array("{sv}".into(), vec![entry]),
	];
	let serial = client_tx.send(ping.clone()).unwrap();

	let call = loop {
		let message = server_rx.receive().unwrap();
		if message.member.as_deref() == Some("Ping") {
			break message;
		}
	};
	assert_eq!(call.body, ping.body);
	let mut reply = Message::method_call("", "/", "", "");
	(reply.kind, reply.path, reply.interface, reply.member) = (Kind::Return, None, None, None);
	reply.destination = call.sender.clone();
	reply.reply_serial = Some(call.serial);
	reply.body = vec![Value::U32(42)];
	server_tx.send(reply).unwrap();
	let mut signal = Message::method_call("", "/test", "org.poopterm.Test", "Pinged");
	(signal.kind, signal.destination) = (Kind::Signal, None);
	signal.body = vec![Value::U32(42), Value::Str("default".into())];
	server_tx.send(signal).unwrap();

	let mut seen = Vec::new();
	while seen.len() < 2 {
		let message = client_rx.receive().unwrap();
		if message.reply_serial == Some(serial) || message.member.as_deref() == Some("Pinged") {
			seen.push(message.body);
		}
	}
	assert_eq!(
		seen,
		[
			vec![Value::U32(42)],
			vec![Value::U32(42), Value::Str("default".into())]
		]
	);
}
