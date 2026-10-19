//! The D-Bus wire format: byte layouts from the specification, round trips
//! and the limits on untrusted messages.
use super::{
	message::{Kind, Message, frame_len},
	wire::{Value, decode_values, encode_values},
};

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn values_are_aligned_as_the_specification_says() {
	// u32, then an array of dict entries: the array length excludes the
	// padding before the first element, entries align to 8.
	let entry = Value::DictEntry(
		Box::new(Value::Str("a".into())),
		Box::new(Value::Variant(Box::new(Value::Byte(2)))),
	);
	let values = [Value::U32(1), Value::Array("{sv}".into(), vec![entry])];
	let bytes = encode_values(&values);
	assert_eq!(hex(&bytes), "010000000a00000001000000610001790002");
	assert_eq!(decode_values("ua{sv}", &bytes, false).unwrap(), values);
	// Eight-byte alignment before an empty array's elements is still written.
	let mut values: Vec<Value> = (1..=5).map(Value::Byte).collect();
	values.push(Value::Array("t".into(), vec![]));
	let bytes = encode_values(&values);
	assert_eq!(hex(&bytes), "01020304050000000000000000000000");
	assert_eq!(decode_values("yyyyyat", &bytes, false).unwrap(), values);
}

#[test]
fn big_endian_values_decode() {
	let bytes = [0, 0, 0, 7, 0, 0, 0, 2, b'h', b'i', 0];
	assert_eq!(
		decode_values("us", &bytes, true).unwrap(),
		[Value::U32(7), Value::Str("hi".into())]
	);
}

#[test]
fn messages_round_trip_with_their_header_fields() {
	let mut call = Message::method_call(
		"org.freedesktop.Notifications",
		"/org/freedesktop/Notifications",
		"org.freedesktop.Notifications",
		"Notify",
	);
	call.serial = 9;
	call.body = vec![Value::Str("app".into()), Value::U32(0), Value::I32(-1)];
	let bytes = call.encode();
	assert_eq!(frame_len(&bytes[..16]), Some(bytes.len()));
	let decoded = Message::decode(&bytes).unwrap();
	assert_eq!(decoded.kind, Kind::MethodCall);
	assert_eq!(decoded.serial, 9);
	assert_eq!(decoded.member.as_deref(), Some("Notify"));
	assert_eq!(
		decoded.interface.as_deref(),
		Some("org.freedesktop.Notifications")
	);
	assert_eq!(
		decoded.path.as_deref(),
		Some("/org/freedesktop/Notifications")
	);
	assert_eq!(
		decoded.destination.as_deref(),
		Some("org.freedesktop.Notifications")
	);
	assert_eq!(decoded.body, call.body);
}

#[test]
fn hello_matches_the_specification_layout() {
	let mut hello = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"Hello",
	);
	hello.serial = 1;
	let bytes = hello.encode();
	// Fixed header, then the field array (path, interface, member and
	// destination, each at an 8-byte boundary): 109 bytes, padded to 128.
	assert_eq!(hex(&bytes[..16]), "6c01000100000000010000006d000000");
	assert_eq!(bytes.len(), 128);
	// PATH: code 1, signature "o", then the string at the next 4-byte boundary.
	assert_eq!(hex(&bytes[16..24]), "01016f0015000000");
	assert_eq!(&bytes[24..45], b"/org/freedesktop/DBus");
}

#[test]
fn untrusted_messages_are_bounded() {
	let mut message = Message::method_call("a.b", "/", "a.b", "M");
	message.body = vec![Value::Str("x".into())];
	let bytes = message.encode();
	// Truncated input, a bad endianness byte, and oversized lengths fail.
	for end in 0..bytes.len() {
		assert!(Message::decode(&bytes[..end]).is_err(), "{end}");
	}
	let mut bad = bytes.clone();
	bad[0] = b'x';
	assert!(Message::decode(&bad).is_err());
	let mut huge = bytes.clone();
	huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
	assert_eq!(frame_len(&huge[..16]), None);
	// Strings must be UTF-8 without NUL, and end in NUL.
	assert!(decode_values("s", &[1, 0, 0, 0, 0xff, 0], false).is_err());
	assert!(decode_values("s", &[1, 0, 0, 0, b'a', 1], false).is_err());
	assert!(decode_values("s", &[1, 0, 0, 0, 0, 0], false).is_err());
	// Arrays cannot claim more bytes than there are.
	assert!(decode_values("ay", &[9, 0, 0, 0, 1], false).is_err());
	// Nesting deeper than the specification allows is refused.
	let deep = format!("{}y", "a".repeat(40));
	assert!(decode_values(&deep, &[0, 0, 0, 0], false).is_err());
	// Malformed signatures are refused.
	for signature in ["(", "a", "{sv}", "a{vs}", "z", "(y"] {
		assert!(
			decode_values(signature, &[0; 16], false).is_err(),
			"{signature}"
		);
	}
	// Variants are nested values, bounded the same way.
	let mut nested = vec![1, b'v', 0];
	for _ in 0..80 {
		nested.extend([1, b'v', 0]);
	}
	assert!(decode_values("v", &nested, false).is_err());
}
